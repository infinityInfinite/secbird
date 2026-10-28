//! Windows UVC controls through the inbox driver, owned by a single COM thread.
use super::{
    camera_id,
    controls::{Control, ControlInfo, ControlState, ControlValue},
    is_kiyo_pro_path, protocol, worker, CameraBackend, CameraInfo, Setting, KIYO_PRO_PID,
};
use crate::{Error, Result};
use std::{
    collections::BTreeMap,
    mem::size_of,
    sync::mpsc::{self, Sender, SyncSender},
    thread,
};
use windows::{
    core::{w, Interface, BSTR, GUID, PCWSTR},
    Win32::{
        Media::{
            DirectShow::{IBaseFilter, ICreateDevEnum},
            KernelStreaming::*,
        },
        System::{
            Com::{StructuredStorage::IPropertyBag, *},
            Variant::VARIANT,
        },
    },
};

const DEVICE_ENUM: GUID = GUID::from_u128(0x62be5d10_60eb_11d0_bd3b_00a0c911ce86);
const VIDEO_CATEGORY: GUID = GUID::from_u128(0x860bb310_5d01_11d0_bd3b_00a0c911ce86);
const EXTENSION: GUID = GUID::from_u128(0x23e49ed0_1178_4f31_ae52_d2fb8a8d3b48);

enum Request {
    Discover(Sender<Result<Vec<CameraInfo>>>),
    Set(String, Setting, Sender<Result<()>>),
    Controls(String, Sender<Result<Vec<ControlInfo>>>),
    SetControl(String, Control, ControlValue, Sender<Result<ControlState>>),
}
struct Worker {
    sender: Option<SyncSender<Request>>,
    thread: Option<thread::JoinHandle<()>>,
}

impl CameraBackend for Worker {
    fn discover(&self) -> Result<Vec<CameraInfo>> {
        worker::call(
            self.sender.as_ref().expect("live worker"),
            Request::Discover,
        )
    }
    fn set(&self, id: &str, setting: Setting) -> Result<()> {
        worker::call(self.sender.as_ref().expect("live worker"), |reply| {
            Request::Set(id.into(), setting, reply)
        })
    }
    fn controls(&self, id: &str) -> Result<Vec<ControlInfo>> {
        worker::call(self.sender.as_ref().expect("live worker"), |reply| {
            Request::Controls(id.into(), reply)
        })
    }
    fn set_control(&self, id: &str, control: Control, value: ControlValue) -> Result<ControlState> {
        worker::call(self.sender.as_ref().expect("live worker"), |reply| {
            Request::SetControl(id.into(), control, value, reply)
        })
    }
}
impl Drop for Worker {
    fn drop(&mut self) {
        self.sender.take();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

pub(super) fn system(context: &str, error: windows::core::Error) -> Error {
    Error::System(format!("camera {context}: {error} (HRESULT {:08X}); check camera access or close other camera apps", error.code().0 as u32))
}

pub(super) fn start() -> Result<Box<dyn CameraBackend>> {
    let (sender, receiver) = mpsc::sync_channel(16);
    let (ready, initialized) = mpsc::channel();
    let thread = thread::Builder::new()
        .name("secbird-camera".into())
        .spawn(move || {
            // SAFETY: this new thread owns all COM initialization and objects below.
            let initialized = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED).ok() };
            if let Err(error) = initialized {
                let _ = ready.send(Err(system("initialize COM", error)));
                return;
            }
            let _ = ready.send(Ok(()));
            {
                let mut cameras = BTreeMap::<String, NativeCamera>::new();
                for request in receiver {
                    match request {
                        Request::Discover(reply) => {
                            cameras.clear();
                            let result = enumerate(&mut cameras);
                            let _ = reply.send(result);
                        }
                        Request::Set(id, setting, reply) => {
                            let result = cameras
                                .get(&id)
                                .ok_or_else(|| {
                                    Error::NotFound("camera disconnected; rescan devices".into())
                                })
                                .and_then(|camera| {
                                    let vendor = camera
                                        .vendor
                                        .as_ref()
                                        .map_err(|error| Error::Unsupported(error.clone()))?;
                                    protocol::send_setting(setting, |payload| vendor.write(payload))
                                });
                            if let Err(error) = &result {
                                if let Some(camera) = cameras.get_mut(&id) {
                                    camera.vendor = Err(error.to_string());
                                }
                            }
                            let _ = reply.send(result);
                        }
                        Request::Controls(id, reply) => {
                            let result = cameras
                                .get(&id)
                                .map(|camera| super::windows_controls::list(&camera.filter))
                                .ok_or_else(|| {
                                    Error::NotFound("camera disconnected; rescan devices".into())
                                });
                            let _ = reply.send(result);
                        }
                        Request::SetControl(id, control, value, reply) => {
                            let result = cameras
                                .get(&id)
                                .ok_or_else(|| {
                                    Error::NotFound("camera disconnected; rescan devices".into())
                                })
                                .and_then(|camera| {
                                    super::windows_controls::set(&camera.filter, control, value)
                                });
                            let _ = reply.send(result);
                        }
                    }
                }
            } // Release filters and node interfaces before CoUninitialize.
            unsafe {
                CoUninitialize();
            }
        })
        .map_err(|e| Error::System(format!("start camera worker: {e}")))?;
    match initialized.recv() {
        Ok(Ok(())) => Ok(Box::new(Worker {
            sender: Some(sender),
            thread: Some(thread),
        })),
        other => {
            drop(sender);
            let _ = thread.join();
            Err(match other {
                Ok(Err(error)) => error,
                _ => Error::System("camera initialization worker stopped".into()),
            })
        }
    }
}

fn property(bag: &IPropertyBag, key: PCWSTR) -> windows::core::Result<String> {
    let mut value = VARIANT::default();
    unsafe {
        bag.Read(key, &mut value, None)?;
    }
    Ok(BSTR::try_from(&value)?.to_string())
}

fn enumerate(cameras: &mut BTreeMap<String, NativeCamera>) -> Result<Vec<CameraInfo>> {
    // SAFETY: called only on the initialized worker; all returned objects remain there.
    unsafe {
        let enumerator: ICreateDevEnum = CoCreateInstance(&DEVICE_ENUM, None, CLSCTX_INPROC_SERVER)
            .map_err(|e| system("enumerate video devices", e))?;
        let mut monikers = None;
        enumerator
            .CreateClassEnumerator(&VIDEO_CATEGORY, &mut monikers, 0)
            .map_err(|e| system("enumerate video inputs", e))?;
        let Some(monikers) = monikers else {
            return Ok(Vec::new());
        };
        let mut found = BTreeMap::new();
        loop {
            let mut next = [None];
            let status = monikers.Next(&mut next, None);
            if status.is_err() {
                return Err(system(
                    "read video input",
                    windows::core::Error::from_hresult(status),
                ));
            }
            let Some(moniker) = next[0].take() else {
                break;
            };
            let Ok(bag) = moniker.BindToStorage::<_, _, IPropertyBag>(None, None) else {
                continue;
            };
            let Ok(path) = property(&bag, w!("DevicePath")) else {
                continue;
            };
            if !is_kiyo_pro_path(&path) {
                continue;
            }
            let id = camera_id(&path);
            if found.contains_key(&id) {
                continue;
            }
            let native = moniker
                .BindToObject::<_, _, IBaseFilter>(None, None)
                .map_err(|e| system("open Kiyo Pro", e));
            let control_error = match native {
                Ok(filter) => {
                    let vendor =
                        VendorControl::open(filter.clone()).map_err(|error| error.to_string());
                    let error = vendor.as_ref().err().cloned();
                    cameras.insert(id.clone(), NativeCamera { filter, vendor });
                    error
                }
                Err(e) => Some(e.to_string()),
            };
            found.insert(
                id.clone(),
                CameraInfo {
                    id,
                    pid: KIYO_PRO_PID,
                    name: "Razer Kiyo Pro".into(),
                    control_error,
                },
            );
        }
        Ok(found.into_values().collect())
    }
}

struct NativeCamera {
    filter: IBaseFilter,
    vendor: std::result::Result<VendorControl, String>,
}

struct VendorControl {
    control: IKsControl,
    node: u32,
}

fn header(node: u32, flags: u32) -> KSP_NODE {
    KSP_NODE {
        Property: KSIDENTIFIER {
            Anonymous: KSIDENTIFIER_0 {
                Anonymous: KSIDENTIFIER_0_0 {
                    Set: EXTENSION,
                    Id: 1,
                    Flags: flags | KSPROPERTY_TYPE_TOPOLOGY,
                },
            },
        },
        NodeId: node,
        Reserved: 0,
    }
}

fn validate_metadata(access: u32, returned: u32, length: u32) -> Result<()> {
    if returned != 4 || access & KSPROPERTY_TYPE_SET == 0 || length != 8 {
        return Err(Error::Unsupported("Kiyo Pro extension selector 1 is missing, read-only, or has an unexpected payload size".into()));
    }
    Ok(())
}

impl VendorControl {
    fn open(filter: IBaseFilter) -> Result<Self> {
        unsafe {
            let topology: IKsTopologyInfo = filter
                .cast()
                .map_err(|e| system("query camera topology", e))?;
            let count = topology
                .NumNodes()
                .map_err(|e| system("count camera nodes", e))?;
            if count > 256 {
                return Err(Error::Unsupported(
                    "camera returned too many topology nodes".into(),
                ));
            }
            for node in 0..count {
                if topology
                    .get_NodeType(node)
                    .map_err(|e| system("read node type", e))?
                    != KSNODETYPE_DEV_SPECIFIC
                {
                    continue;
                }
                let mut raw = std::ptr::null_mut();
                if topology
                    .CreateNodeInstance(node, &IKsControl::IID, &mut raw)
                    .is_err()
                    || raw.is_null()
                {
                    continue;
                }
                let control = IKsControl::from_raw(raw);
                let camera = Self { control, node };
                if camera.validate().is_ok() {
                    return Ok(camera);
                }
            }
        }
        Err(Error::Unsupported(
            "Kiyo Pro UVC extension controls unavailable; check camera permissions, then rescan"
                .into(),
        ))
    }

    fn validate(&self) -> Result<()> {
        let mut access = 0u32;
        let mut returned = 0;
        self.query(
            KSPROPERTY_TYPE_BASICSUPPORT,
            (&mut access as *mut u32).cast(),
            4,
            &mut returned,
        )
        .map_err(|e| system("query extension support", e))?;
        let mut length = 0;
        // A zero-length GET queries required size, following Microsoft's UVC sample.
        let size = self.query(KSPROPERTY_TYPE_GET, std::ptr::null_mut(), 0, &mut length);
        match size {
            Ok(()) => {}
            Err(error) if error.code().0 as u32 == 0x800700ea => {} // ERROR_MORE_DATA
            Err(error) => return Err(system("query extension length", error)),
        }
        validate_metadata(access, returned, length)
    }

    fn query(
        &self,
        flags: u32,
        data: *mut std::ffi::c_void,
        size: u32,
        returned: &mut u32,
    ) -> windows::core::Result<()> {
        let property = header(self.node, flags);
        // SAFETY: property and data live through this synchronous call. Caller supplies
        // a buffer of size bytes (or null for the documented size query).
        unsafe {
            self.control.KsProperty(
                &property.Property,
                size_of::<KSP_NODE>() as u32,
                data,
                size,
                returned,
            )
        }
    }

    fn write(&self, payload: &[u8; 8]) -> Result<()> {
        let mut data = *payload;
        let mut returned = 0;
        self.query(
            KSPROPERTY_TYPE_SET,
            data.as_mut_ptr().cast(),
            8,
            &mut returned,
        )
        .map_err(|e| system("write extension control", e))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn camera_rejects_malformed_or_readonly_metadata() {
        assert!(validate_metadata(KSPROPERTY_TYPE_SET, 4, 8).is_ok());
        for (access, bytes, size) in [
            (0, 4, 8),
            (KSPROPERTY_TYPE_SET, 0, 8),
            (KSPROPERTY_TYPE_SET, 4, 9),
        ] {
            assert!(validate_metadata(access, bytes, size).is_err());
        }
    }
}
