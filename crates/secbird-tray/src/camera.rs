//! Camera editor schema and dispatch; standard controls have native readbacks.
use secbird_core::{
    camera::{
        actions,
        controls::{parse_control, Control as StandardControl},
        parse_setting, CameraBackend, CameraInfo,
    },
    Error, Result, Store,
};
use serde::Deserialize;
use serde_json::{json, Value};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Control {
    id: String,
    control: String,
    value: String,
    #[serde(default)]
    profile: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReadControls {
    id: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ProfileRequest {
    id: String,
    action: String,
    name: String,
    #[serde(default)]
    apps: Vec<String>,
}

pub(crate) fn device_json(
    camera: &CameraInfo,
    store: &mut Store,
    foreground: Option<&str>,
) -> Value {
    let profiles = store.for_camera(&camera.id);
    let active = profiles.effective(foreground);
    json!({
        "id": camera.id, "pid": format!("{:04X}", camera.pid), "name": camera.name,
        "kind": "webcam", "known": true, "leds": [], "macros": false, "lighting_control": false,
        "profiles": profiles.profiles.keys().collect::<Vec<_>>(), "active": active,
        "manual_active": profiles.active, "profile": profiles.profiles.get(active),
        "profilesData": profiles.profiles, "camera": { "state": "saved", "error": camera.control_error },
    })
}

pub(crate) fn label(camera: &CameraInfo, cameras: &[CameraInfo]) -> String {
    if cameras
        .iter()
        .filter(|other| other.pid == camera.pid)
        .count()
        > 1
    {
        let index = cameras
            .iter()
            .position(|other| other.id == camera.id)
            .unwrap_or(0)
            + 1;
        format!("{} ({index})", camera.name)
    } else {
        camera.name.clone()
    }
}

fn labeled_device_json(
    camera: &CameraInfo,
    cameras: &[CameraInfo],
    store: &mut Store,
    foreground: Option<&str>,
) -> Value {
    let mut value = device_json(camera, store, foreground);
    value["label"] = label(camera, cameras).into();
    value
}

pub(crate) fn device_list_json(
    cameras: &[CameraInfo],
    store: &mut Store,
    foreground: Option<&str>,
) -> Vec<Value> {
    cameras
        .iter()
        .map(|camera| labeled_device_json(camera, cameras, store, foreground))
        .collect()
}

fn dispatch(
    backend: &dyn CameraBackend,
    cameras: &[CameraInfo],
    foreground: Option<&str>,
    path: &str,
    body: &[u8],
    store: &mut Store,
) -> Result<Value> {
    let invalid = |error: serde_json::Error| Error::InvalidArg(error.to_string());
    let find = |id: &str| {
        cameras
            .iter()
            .find(|camera| camera.id == id)
            .ok_or_else(|| Error::NotFound("camera identity not found; rescan devices".into()))
    };
    match path {
        "/api/camera-controls" => {
            let request: ReadControls = serde_json::from_slice(body).map_err(invalid)?;
            let camera = find(&request.id)?;
            Ok(json!({"controls":backend.controls(&camera.id)?}))
        }
        "/api/camera" => {
            let request: Control = serde_json::from_slice(body).map_err(invalid)?;
            let camera = find(&request.id)?;
            let name = store
                .for_camera(&camera.id)
                .effective(foreground)
                .to_owned();
            if request
                .profile
                .as_ref()
                .is_some_and(|expected| *expected != name)
            {
                return Err(Error::InvalidArg(
                    "effective camera profile changed; refresh before editing".into(),
                ));
            }
            if request.control.parse::<StandardControl>().is_ok() {
                let (control, value) = parse_control(&request.control, &request.value)?;
                let state =
                    actions::set_control(backend, &camera.id, store, &name, control, value)?;
                let controls = backend.controls(&camera.id).ok();
                return Ok(
                    json!({"device":labeled_device_json(camera,cameras,store,foreground),"written_state":state,
                    "controls":controls,"failures":[]}),
                );
            }
            let setting = parse_setting(&request.control, &request.value)?;
            actions::set(backend, &camera.id, store, &name, setting)?;
            Ok(
                json!({ "device": labeled_device_json(camera, cameras, store, foreground), "failures": [] }),
            )
        }
        "/api/camera-profile" => {
            let request: ProfileRequest = serde_json::from_slice(body).map_err(invalid)?;
            let camera = find(&request.id)?;
            let profiles = store.for_camera(&camera.id);
            match request.action.as_str() {
                "new" => profiles.create(request.name.trim())?,
                "delete" => profiles.delete(&request.name)?,
                "use" => profiles.switch(&request.name)?,
                "apps" => profiles.set_apps(&request.name, &request.apps)?,
                _ => return Err(Error::InvalidArg("unknown camera profile action".into())),
            }
            let effective = profiles.effective(foreground).to_owned();
            let profile = profiles.profiles[&effective].clone();
            store.save()?;
            let failures = if matches!(request.action.as_str(), "use" | "apps" | "delete") {
                actions::apply(backend, &camera.id, &profile)
                    .into_iter()
                    .filter_map(|item| item.result.err().map(|e| format!("{}: {e}", item.setting)))
                    .collect::<Vec<_>>()
            } else {
                Vec::new()
            };
            Ok(
                json!({ "device": labeled_device_json(camera, cameras, store, foreground), "failures": failures }),
            )
        }
        _ => Err(Error::NotFound(path.into())),
    }
}

#[cfg(windows)]
pub(crate) fn post(
    engine: &mut crate::engine::Engine,
    path: &str,
    body: &[u8],
) -> Option<Result<Value>> {
    if !matches!(
        path,
        "/api/camera" | "/api/camera-profile" | "/api/camera-controls"
    ) {
        return None;
    }
    Some((|| {
        let mut store = Store::load(&secbird_core::config_dir().join("profiles.json"))?;
        dispatch(
            engine.camera_backend(),
            engine.cameras(),
            engine.foreground(),
            path,
            body,
            &mut store,
        )
    })())
}

#[cfg(test)]
mod tests {
    use super::*;
    use secbird_core::camera::{CameraBackend, CameraInfo, Setting};
    use std::sync::Mutex;

    #[test]
    fn standard_camera_api_does_not_write_for_wrong_target_or_stale_profile() {
        use secbird_core::camera::controls::{Control, ControlInfo, ControlState, ControlValue};
        struct Fake(Mutex<u32>);
        impl CameraBackend for Fake {
            fn discover(&self) -> Result<Vec<CameraInfo>> {
                Ok(vec![])
            }
            fn set(&self, _: &str, _: Setting) -> Result<()> {
                panic!("unexpected vendor write")
            }
            fn controls(&self, _: &str) -> Result<Vec<ControlInfo>> {
                Ok(vec![ControlInfo::new(
                    Control::Exposure,
                    None,
                    None,
                    Some("unavailable".into()),
                )])
            }
            fn set_control(&self, _: &str, _: Control, _: ControlValue) -> Result<ControlState> {
                *self.0.lock().unwrap() += 1;
                Err(Error::System("driver rejected".into()))
            }
        }
        let backend = Fake(Mutex::new(0));
        let mut store = Store::default();
        let cameras = [camera()];
        assert_eq!(
            dispatch(
                &backend,
                &cameras,
                None,
                "/api/camera-controls",
                br#"{"id":"camera:test"}"#,
                &mut store
            )
            .unwrap()["controls"][0]["control"],
            "exposure"
        );
        for json in [
            br#"{"id":"0099","control":"exposure","value":"-6"}"#.as_slice(),
            br#"{"id":"camera:test","control":"exposure","value":"-6","profile":"stale"}"#,
            br#"{"id":"camera:test","control":"exposure","value":"nope"}"#,
        ] {
            assert!(dispatch(&backend, &cameras, None, "/api/camera", json, &mut store).is_err());
        }
        assert_eq!(*backend.0.lock().unwrap(), 0);
        assert!(store.for_camera("camera:test").active().camera.is_none());
    }

    #[derive(Default)]
    struct Fake(Mutex<Vec<Setting>>);
    impl CameraBackend for Fake {
        fn discover(&self) -> secbird_core::Result<Vec<CameraInfo>> {
            Ok(Vec::new())
        }
        fn set(&self, _: &str, value: Setting) -> secbird_core::Result<()> {
            self.0.lock().unwrap().push(value);
            Ok(())
        }
    }
    fn camera() -> CameraInfo {
        CameraInfo {
            id: "camera:test".into(),
            pid: 0x0e05,
            name: "Razer Kiyo Pro".into(),
            control_error: None,
        }
    }

    #[test]
    fn camera_api_rejects_invalid_requests_without_writes() {
        let fake = Fake::default();
        let mut store = secbird_core::Store::default();
        for body in [
            r#"{"id":"","control":"hdr","value":"on"}"#,
            r#"{"id":"camera:test","control":"hdr","value":"yes"}"#,
            r#"{"id":"camera:test","control":"hdr","value":"on","extra":1}"#,
            r#"{"id":"0099","control":"hdr","value":"on"}"#,
        ] {
            assert!(dispatch(
                &fake,
                &[camera()],
                None,
                "/api/camera",
                body.as_bytes(),
                &mut store
            )
            .is_err());
        }
        assert!(fake.0.lock().unwrap().is_empty());
    }

    #[test]
    fn camera_json_uses_effective_profile_without_peripheral_probes() {
        let mut store = secbird_core::Store::default();
        let profiles = store.for_camera("camera:test");
        profiles.create("Streaming").unwrap();
        profiles.set_apps("Streaming", &["obs.exe".into()]).unwrap();
        let value = device_json(&camera(), &mut store, Some("obs.exe"));
        assert_eq!(value["active"], "Streaming");
        assert_eq!(value["manual_active"], "Default");
        assert_eq!(value["kind"], "webcam");
        assert_eq!(value["leds"], serde_json::json!([]));
        assert!(value["firmware"].is_null());
        assert_eq!(value["camera"]["state"], "saved");
    }

    #[test]
    fn camera_twins_have_distinct_labels_and_inspectable_local_identity() {
        let first = camera();
        let mut second = camera();
        second.id = "camera:second".into();
        let values = device_list_json(&[first, second], &mut Store::default(), None);
        assert_eq!(values[0]["label"], "Razer Kiyo Pro (1)");
        assert_eq!(values[1]["label"], "Razer Kiyo Pro (2)");
        assert_eq!(values[0]["id"], "camera:test");
        assert_eq!(values[1]["id"], "camera:second");
    }

    #[test]
    fn camera_api_writes_effective_profile_and_rejects_stale_profile() {
        let path = std::env::temp_dir().join(format!(
            "secbird-api-{}-{}.json",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let mut store = Store::load(&path).unwrap();
        let fake = Fake::default();
        let profiles = store.for_camera("camera:test");
        profiles.create("Streaming").unwrap();
        profiles.set_apps("Streaming", &["obs.exe".into()]).unwrap();
        let cameras = [camera()];
        let result = dispatch(
            &fake,
            &cameras,
            Some("obs.exe"),
            "/api/camera",
            br#"{"id":"camera:test","control":"fov","value":"wide","profile":"Streaming"}"#,
            &mut store,
        )
        .unwrap();
        assert_eq!(result["device"]["profile"]["camera"]["fov"], "wide");
        assert!(store.for_camera("camera:test").active().camera.is_none());
        assert!(dispatch(
            &fake,
            &cameras,
            Some("obs.exe"),
            "/api/camera",
            br#"{"id":"camera:test","control":"hdr","value":"on","profile":"Default"}"#,
            &mut store
        )
        .is_err());
        assert_eq!(fake.0.lock().unwrap().len(), 1);
        std::fs::remove_file(path).unwrap();
    }
}
