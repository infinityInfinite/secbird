use crate::{Error, Result};
use serde::{Deserialize, Serialize};
use std::str::FromStr;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Control {
    Brightness,
    Contrast,
    Hue,
    Saturation,
    Sharpness,
    Gamma,
    WhiteBalance,
    BacklightCompensation,
    Gain,
    ColorEnable,
    #[serde(rename = "anti-flicker")]
    PowerlineFrequency,
    Zoom,
    Pan,
    Tilt,
    Focus,
    Exposure,
    #[serde(rename = "aperture")]
    Iris,
    Roll,
    LowLightCompensation,
}
impl Control {
    pub const ALL: [Self; 19] = [
        Self::Brightness,
        Self::Contrast,
        Self::Hue,
        Self::Saturation,
        Self::Sharpness,
        Self::Gamma,
        Self::WhiteBalance,
        Self::BacklightCompensation,
        Self::Gain,
        Self::ColorEnable,
        Self::PowerlineFrequency,
        Self::Zoom,
        Self::Pan,
        Self::Tilt,
        Self::Focus,
        Self::Exposure,
        Self::Iris,
        Self::Roll,
        Self::LowLightCompensation,
    ];
    pub fn name(self) -> &'static str {
        match self {
            Self::Brightness => "brightness",
            Self::Contrast => "contrast",
            Self::Hue => "hue",
            Self::Saturation => "saturation",
            Self::Sharpness => "sharpness",
            Self::Gamma => "gamma",
            Self::WhiteBalance => "white-balance",
            Self::BacklightCompensation => "backlight-compensation",
            Self::Gain => "gain",
            Self::ColorEnable => "color-enable",
            Self::PowerlineFrequency => "anti-flicker",
            Self::Zoom => "zoom",
            Self::Pan => "pan",
            Self::Tilt => "tilt",
            Self::Focus => "focus",
            Self::Exposure => "exposure",
            Self::Iris => "aperture",
            Self::Roll => "roll",
            Self::LowLightCompensation => "low-light-compensation",
        }
    }
    pub fn group(self) -> &'static str {
        if Self::ALL[..11].contains(&self) {
            "image"
        } else {
            "camera"
        }
    }
    pub fn kind(self) -> &'static str {
        match self {
            Self::PowerlineFrequency => "frequency",
            Self::ColorEnable | Self::LowLightCompensation => "toggle",
            _ => "range",
        }
    }
}
impl FromStr for Control {
    type Err = Error;
    fn from_str(name: &str) -> Result<Self> {
        Self::ALL
            .into_iter()
            .find(|control| control.name() == name)
            .ok_or_else(|| Error::InvalidArg(format!("unknown camera control '{name}'")))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "mode", content = "value", rename_all = "lowercase")]
pub enum ControlValue {
    Manual(i32),
    Auto,
    Default,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct ControlState {
    pub value: i32,
    pub automatic: bool,
}
impl ControlState {
    pub fn saved(self) -> ControlValue {
        if self.automatic {
            ControlValue::Auto
        } else {
            ControlValue::Manual(self.value)
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ControlRange {
    pub min: i32,
    pub max: i32,
    pub step: i32,
    pub default: Option<i32>,
    pub automatic: bool,
    pub manual: bool,
    pub choices: Vec<i32>,
}
impl ControlRange {
    pub fn accepts(&self, value: i32) -> bool {
        self.step > 0
            && value >= self.min
            && value <= self.max
            && (i64::from(value) - i64::from(self.min)) % i64::from(self.step) == 0
            && (self.choices.is_empty() || self.choices.contains(&value))
    }
    pub fn validate(&self) -> Result<()> {
        if self.min > self.max
            || self.step <= 0
            || (!self.automatic && !self.manual)
            || self.default.is_some_and(|value| !self.accepts(value))
            || self.choices.iter().any(|value| !self.accepts(*value))
        {
            Err(Error::Unsupported(
                "driver returned an invalid control range".into(),
            ))
        } else {
            Ok(())
        }
    }
    pub fn resolve(
        &self,
        requested: ControlValue,
        current: Option<ControlState>,
    ) -> Result<ControlState> {
        self.validate()?;
        match requested {
            ControlValue::Auto if self.automatic => Ok(ControlState {
                value: current
                    .map(|s| s.value)
                    .filter(|v| self.accepts(*v))
                    .or(self.default)
                    .unwrap_or(self.min),
                automatic: true,
            }),
            ControlValue::Manual(value) if self.manual && self.accepts(value) => Ok(ControlState {
                value,
                automatic: false,
            }),
            ControlValue::Default => self
                .default
                .map(|value| ControlState {
                    value,
                    automatic: !self.manual,
                })
                .ok_or_else(|| {
                    Error::Unsupported("driver did not report a default for this control".into())
                }),
            _ => Err(Error::InvalidArg(format!(
                "unsupported control mode/value; range {}..{} step {}",
                self.min, self.max, self.step
            ))),
        }
    }
}
#[derive(Debug, Clone, Serialize)]
pub struct ControlInfo {
    pub control: Control,
    pub group: &'static str,
    pub kind: &'static str,
    pub range: Option<ControlRange>,
    pub current: Option<ControlState>,
    pub error: Option<String>,
}

pub fn validate_prerequisite(
    control: Control,
    requested: ControlState,
    related: Option<i32>,
) -> Result<()> {
    if control == Control::LowLightCompensation
        && requested.value != 0
        && !related.is_some_and(|flags| flags & 1 != 0)
    {
        return Err(Error::InvalidArg(
            "low-light compensation needs automatic exposure; enable Exposure Auto first".into(),
        ));
    }
    if control == Control::Exposure && !requested.automatic && related == Some(1) {
        return Err(Error::InvalidArg(
            "turn off low-light compensation before switching Exposure to Manual".into(),
        ));
    }
    Ok(())
}
impl ControlInfo {
    pub fn new(
        control: Control,
        range: Option<ControlRange>,
        current: Option<ControlState>,
        error: Option<String>,
    ) -> Self {
        Self {
            control,
            group: control.group(),
            kind: control.kind(),
            range,
            current,
            error,
        }
    }
}
pub fn parse_control(name: &str, text: &str) -> Result<(Control, ControlValue)> {
    let control: Control = name.parse()?;
    let value = match (control, text) {
        (_, "default") => ControlValue::Default,
        (Control::PowerlineFrequency, "off") => ControlValue::Manual(0),
        (Control::PowerlineFrequency, "50hz") => ControlValue::Manual(1),
        (Control::PowerlineFrequency, "60hz") => ControlValue::Manual(2),
        (Control::PowerlineFrequency, "auto") => ControlValue::Manual(3),
        (Control::ColorEnable | Control::LowLightCompensation, "off") => ControlValue::Manual(0),
        (Control::ColorEnable | Control::LowLightCompensation, "on") => ControlValue::Manual(1),
        (_, "auto") => ControlValue::Auto,
        _ => ControlValue::Manual(
            text.parse()
                .map_err(|_| Error::InvalidArg(format!("invalid {name} value")))?,
        ),
    };
    Ok((control, value))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn validation_and_modes() {
        let range = ControlRange {
            min: -12,
            max: -2,
            step: 2,
            default: Some(-6),
            automatic: true,
            manual: true,
            choices: vec![],
        };
        assert_eq!(
            range.resolve(ControlValue::Auto, None).unwrap().saved(),
            ControlValue::Auto
        );
        assert_eq!(
            range.resolve(ControlValue::Manual(-6), None).unwrap().value,
            -6
        );
        assert!(range.resolve(ControlValue::Manual(-5), None).is_err());
        assert_eq!(
            range.resolve(ControlValue::Default, None).unwrap().value,
            -6
        );
        assert!(ControlRange {
            default: None,
            ..range.clone()
        }
        .resolve(ControlValue::Default, None)
        .is_err());
        assert!(ControlRange {
            step: 0,
            ..range.clone()
        }
        .validate()
        .is_err());
    }
    #[test]
    fn parse_and_persist_auto_separately_from_strategy() {
        assert_eq!(
            parse_control("focus", "auto").unwrap(),
            (Control::Focus, ControlValue::Auto)
        );
        assert_eq!(
            parse_control("anti-flicker", "60hz").unwrap(),
            (Control::PowerlineFrequency, ControlValue::Manual(2))
        );
        assert_eq!(
            parse_control("exposure", "-6").unwrap(),
            (Control::Exposure, ControlValue::Manual(-6))
        );
        assert!(parse_control("fps", "60").is_err());
        assert_eq!(
            serde_json::to_string(&ControlValue::Auto).unwrap(),
            r#"{"mode":"auto"}"#
        );
    }

    #[test]
    fn exposure_manual_requires_low_light_compensation_off() {
        assert!(validate_prerequisite(
            Control::Exposure,
            ControlState {
                value: -6,
                automatic: false
            },
            Some(1)
        )
        .is_err());
        assert!(validate_prerequisite(
            Control::Exposure,
            ControlState {
                value: -6,
                automatic: false
            },
            Some(0)
        )
        .is_ok());
        assert!(validate_prerequisite(
            Control::Exposure,
            ControlState {
                value: -6,
                automatic: true
            },
            Some(1)
        )
        .is_ok());
        assert!(validate_prerequisite(
            Control::LowLightCompensation,
            ControlState {
                value: 1,
                automatic: false
            },
            Some(0)
        )
        .is_err());
        assert!(validate_prerequisite(
            Control::LowLightCompensation,
            ControlState {
                value: 1,
                automatic: false
            },
            Some(1)
        )
        .is_ok());
    }
}
