//! Standard controls through the inbox DirectShow camera interfaces. Power-line
//! frequency and low-light exposure priority use their KS properties instead.
use super::{
    controls::{
        validate_prerequisite, Control, ControlInfo, ControlRange, ControlState, ControlValue,
    },
    windows::system,
};
use crate::{Error, Result};
use std::mem::{offset_of, size_of};
use windows::{
    core::Interface,
    Win32::Media::{
        DirectShow::{IAMCameraControl, IAMVideoProcAmp, IBaseFilter},
        KernelStreaming::*,
    },
};

fn location(control: Control) -> (bool, i32) {
    match control {
        Control::Brightness => (false, 0),
        Control::Contrast => (false, 1),
        Control::Hue => (false, 2),
        Control::Saturation => (false, 3),
        Control::Sharpness => (false, 4),
        Control::Gamma => (false, 5),
        Control::ColorEnable => (false, 6),
        Control::WhiteBalance => (false, 7),
        Control::BacklightCompensation => (false, 8),
        Control::Gain => (false, 9),
        Control::PowerlineFrequency => (false, 13),
        Control::Pan => (true, 0),
        Control::Tilt => (true, 1),
        Control::Roll => (true, 2),
        Control::Zoom => (true, 3),
        Control::Exposure => (true, 4),
        Control::Iris => (true, 5),
        Control::Focus => (true, 6),
        Control::LowLightCompensation => (true, 19),
    }
}
fn ks_only(control: Control) -> bool {
    matches!(
        control,
        Control::PowerlineFrequency | Control::LowLightCompensation
    )
}

fn ks_property(control: Control, flags: u32) -> KSIDENTIFIER {
    let (camera, id) = location(control);
    KSIDENTIFIER {
        Anonymous: KSIDENTIFIER_0 {
            Anonymous: KSIDENTIFIER_0_0 {
                Set: if camera {
                    PROPSETID_VIDCAP_CAMERACONTROL
                } else {
                    PROPSETID_VIDCAP_VIDEOPROCAMP
                },
                Id: id as u32,
                Flags: flags,
            },
        },
    }
}
fn support(filter: &IBaseFilter, control: Control) -> Result<bool> {
    let ks: IKsControl = filter
        .cast()
        .map_err(|error| system("query camera KS interface", error))?;
    let property = ks_property(control, KSPROPERTY_TYPE_BASICSUPPORT);
    let mut flags = 0u32;
    let mut returned = 0u32;
    // SAFETY: initialized property and output buffers remain live for the COM call.
    unsafe {
        ks.KsProperty(
            &property,
            size_of::<KSIDENTIFIER>() as u32,
            (&mut flags as *mut u32).cast(),
            4,
            &mut returned,
        )
    }
    .map_err(|error| system(&format!("query {} support", control.name()), error))?;
    if returned < 4 {
        return Err(Error::Unsupported(format!(
            "{} support reply is truncated",
            control.name()
        )));
    }
    Ok(flags & KSPROPERTY_TYPE_SET != 0 && flags & KSPROPERTY_TYPE_GET != 0)
}

fn ks_state(
    filter: &IBaseFilter,
    control: Control,
    requested: Option<ControlState>,
) -> Result<ControlState> {
    let ks: IKsControl = filter
        .cast()
        .map_err(|error| system("query camera KS interface", error))?;
    let mut property = KSPROPERTY_CAMERACONTROL_S {
        Property: ks_property(
            control,
            if requested.is_some() {
                KSPROPERTY_TYPE_SET
            } else {
                KSPROPERTY_TYPE_GET
            },
        ),
        Value: requested.map_or(0, |state| state.value),
        Flags: 0,
        Capabilities: 0,
    };
    let mut result = property;
    let mut returned = 0;
    // SAFETY: synchronous property call with valid initialized buffers; these enum
    // controls encode Auto as a numeric value (frequency=3), not an Auto flag.
    unsafe {
        ks.KsProperty(
            (&mut property as *mut KSPROPERTY_CAMERACONTROL_S).cast(),
            size_of::<KSPROPERTY_CAMERACONTROL_S>() as u32,
            (&mut result as *mut KSPROPERTY_CAMERACONTROL_S).cast(),
            size_of::<KSPROPERTY_CAMERACONTROL_S>() as u32,
            &mut returned,
        )
    }
    .map_err(|error| {
        system(
            &format!(
                "{} {}",
                if requested.is_some() { "write" } else { "read" },
                control.name()
            ),
            error,
        )
    })?;
    if requested.is_none()
        && returned < (offset_of!(KSPROPERTY_CAMERACONTROL_S, Value) + size_of::<i32>()) as u32
    {
        return Err(Error::Unsupported(
            "camera KS value reply is truncated".into(),
        ));
    }
    Ok(requested.unwrap_or(ControlState {
        value: result.Value,
        automatic: false,
    }))
}

fn range(filter: &IBaseFilter, control: Control) -> Result<ControlRange> {
    if ks_only(control) {
        if !support(filter, control)? {
            return Err(Error::Unsupported(format!(
                "{} is not readable/writable",
                control.name()
            )));
        }
        let current = ks_state(filter, control, None)?.value;
        // These two controls have fixed values in the Windows UVC specification;
        // don't assume optional frequency Auto(3) unless the driver reports it.
        let choices = if control == Control::LowLightCompensation {
            vec![0, 1]
        } else if current == 3 {
            vec![0, 1, 2, 3]
        } else {
            vec![0, 1, 2]
        };
        let max = *choices.last().expect("fixed control choices");
        let range = ControlRange {
            min: 0,
            max,
            step: 1,
            default: (control == Control::LowLightCompensation).then_some(0),
            automatic: false,
            manual: true,
            choices,
        };
        range.validate()?;
        return Ok(range);
    }
    let (camera, id) = location(control);
    let (mut min, mut max, mut step, mut default, mut flags) = (0, 0, 0, 0, 0);
    // SAFETY: native interfaces and output scalars stay on the initialized worker.
    unsafe {
        if camera {
            filter.cast::<IAMCameraControl>().and_then(|api| {
                api.GetRange(id, &mut min, &mut max, &mut step, &mut default, &mut flags)
            })
        } else {
            filter.cast::<IAMVideoProcAmp>().and_then(|api| {
                api.GetRange(id, &mut min, &mut max, &mut step, &mut default, &mut flags)
            })
        }
    }
    .map_err(|error| system(&format!("query {} range", control.name()), error))?;
    let range = ControlRange {
        min,
        max,
        step,
        default: Some(default),
        automatic: flags & 1 != 0,
        manual: flags & 2 != 0,
        choices: Vec::new(),
    };
    range.validate()?;
    Ok(range)
}

fn read(filter: &IBaseFilter, control: Control) -> Result<ControlState> {
    if ks_only(control) {
        return ks_state(filter, control, None);
    }
    let (camera, id) = location(control);
    let (mut value, mut flags) = (0, 0);
    unsafe {
        if camera {
            filter
                .cast::<IAMCameraControl>()
                .and_then(|api| api.Get(id, &mut value, &mut flags))
        } else {
            filter
                .cast::<IAMVideoProcAmp>()
                .and_then(|api| api.Get(id, &mut value, &mut flags))
        }
    }
    .map_err(|error| system(&format!("read {}", control.name()), error))?;
    if flags & 3 != 1 && flags & 3 != 2 {
        return Err(Error::Unsupported(
            "camera reported an unknown Auto/manual state".into(),
        ));
    }
    Ok(ControlState {
        value,
        automatic: flags & 1 != 0,
    })
}

pub(super) fn list(filter: &IBaseFilter) -> Vec<ControlInfo> {
    Control::ALL
        .into_iter()
        .map(|control| match range(filter, control) {
            Err(error) => ControlInfo::new(control, None, None, Some(error.to_string())),
            Ok(range) => match read(filter, control) {
                Ok(value) => ControlInfo::new(control, Some(range), Some(value), None),
                Err(error) => ControlInfo::new(control, Some(range), None, Some(error.to_string())),
            },
        })
        .collect()
}
pub(super) fn set(
    filter: &IBaseFilter,
    control: Control,
    value: ControlValue,
) -> Result<ControlState> {
    let range = range(filter, control)?;
    let current = read(filter, control).ok();
    let requested = range.resolve(value, current)?;
    if current == Some(requested) {
        return Ok(requested);
    }
    let related = match control {
        Control::LowLightCompensation => read(filter, Control::Exposure)
            .ok()
            .map(|state| i32::from(state.automatic)),
        Control::Exposure => read(filter, Control::LowLightCompensation)
            .ok()
            .map(|state| state.value),
        _ => None,
    };
    validate_prerequisite(control, requested, related)?;
    if ks_only(control) {
        ks_state(filter, control, Some(requested))?;
    } else {
        let (camera, id) = location(control);
        let flags = if requested.automatic { 1 } else { 2 };
        unsafe {
            if camera {
                filter
                    .cast::<IAMCameraControl>()
                    .and_then(|api| api.Set(id, requested.value, flags))
            } else {
                filter
                    .cast::<IAMVideoProcAmp>()
                    .and_then(|api| api.Set(id, requested.value, flags))
            }
        }
        .map_err(|error| system(&format!("write {}", control.name()), error))?;
    }
    let state = read(filter, control).map_err(|error| {
        Error::System(format!(
            "{} may have changed but readback failed: {error}",
            control.name()
        ))
    })?;
    if state.automatic != requested.automatic
        || (!state.automatic && state.value != requested.value)
    {
        return Err(Error::System(format!(
            "{} may have changed but readback differs",
            control.name()
        )));
    }
    Ok(state)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_standard_camera_ids_match_windows_sdk() {
        assert_eq!(location(Control::Focus), (true, 6));
        assert_eq!(
            location(Control::PowerlineFrequency),
            (false, KSPROPERTY_VIDEOPROCAMP_POWERLINE_FREQUENCY.0)
        );
        assert_eq!(
            location(Control::LowLightCompensation),
            (true, KSPROPERTY_CAMERACONTROL_AUTO_EXPOSURE_PRIORITY.0)
        );
        assert_eq!(size_of::<KSPROPERTY_CAMERACONTROL_S>(), 36);
    }
}
