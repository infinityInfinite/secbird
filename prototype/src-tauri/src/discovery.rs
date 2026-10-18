//! Passive OS HID enumeration only. Do NOT use core::discover: it probes via feature reports.
use secbird_core::{hidapi::HidApi, Db};
use serde::Serialize;
use std::collections::BTreeMap;
use std::hash::{Hash, Hasher};

#[derive(Clone, Serialize)]
#[serde(rename_all="camelCase")]
pub struct DetectedDevice {
    pub id: String,
    pub name: String,
    pub pid: String,
    pub kind: String,
    pub connection: String,
    pub interfaces: usize,
    pub identity_quality: String,
    pub dpi: Option<Limits>,
    pub polling: Vec<u32>,
    pub lighting: bool,
}
#[derive(Clone, Serialize)]
pub struct Limits { min: u16, max: u16, step: u16 }

pub fn enumerate() -> Result<Vec<DetectedDevice>, String> {
    let api=HidApi::new().map_err(|e| format!("HID enumeration failed: {e}"))?;
    let db=Db::builtin().map_err(|e| e.to_string())?;
    let mut found: BTreeMap<String,DetectedDevice>=BTreeMap::new();
    for info in api.device_list().filter(|d|d.vendor_id()==0x1532) {
        // get_container_id reads a Windows PnP property. No feature/input/output report is sent.
        let container=info.open_device(&api).ok().and_then(|h|h.get_container_id().ok())
            .map(|g|format!("{:08x}-{:04x}-{:04x}-{:02x?}",g.data1,g.data2,g.data3,g.data4));
        let serial=info.serial_number().filter(|s|!s.trim().is_empty());
        let (identity,quality)=if let Some(value)=container { (value,"container") }
            else if let Some(value)=serial { (value.to_owned(),"serial") }
            else { (format!("pid:{:04x}",info.product_id()),"pid-only") };
        // No raw serial/path leaves this process. Include PID to distinguish paired endpoints.
        let mut hash=std::collections::hash_map::DefaultHasher::new();
        identity.hash(&mut hash);info.product_id().hash(&mut hash);
        let id=format!("razer-{:04x}-{:016x}",info.product_id(),hash.finish());
        if let Some(row)=found.get_mut(&id){row.interfaces+=1;continue;}
        let spec=db.get(info.product_id());
        let name=spec.map(|s|s.name.clone()).or_else(||info.product_string().map(str::to_owned))
            .unwrap_or_else(||format!("Razer USB {:04X}",info.product_id()));
        let wireless=name.contains("Wireless")||name.contains("Dongle")||name.contains("Receiver");
        found.insert(id.clone(),DetectedDevice {
            id,name,pid:format!("{:04X}",info.product_id()),
            kind:spec.map(|s|format!("{:?}",s.kind).to_lowercase()).unwrap_or_else(||"unknown".into()),
            connection:if wireless {"USB receiver detected · radio connection unverified"}else{"USB HID detected"}.into(),
            interfaces:1,identity_quality:quality.into(),
            dpi:spec.and_then(|s|s.dpi.as_ref()).map(|d|Limits{min:d.min,max:d.max,step:d.step}),
            polling:spec.and_then(|s|s.polling).map(|p|match p {secbird_core::db::Polling::High=>vec![125,250,500,1000,2000,4000,8000],_=>vec![125,500,1000]}).unwrap_or_default(),
            lighting:spec.is_some_and(|s|!s.leds.is_empty()||s.matrix.is_some()),
        });
    }
    Ok(found.into_values().collect())
}
