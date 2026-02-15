use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// The type of effect applied to a clip.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EffectType {
    Transform,
    Grayscale,
    Brightness,
    Opacity,
}

impl EffectType {
    /// Human-readable display name.
    pub fn display_name(&self) -> &'static str {
        match self {
            Self::Transform => "Transform",
            Self::Grayscale => "Grayscale",
            Self::Brightness => "Brightness",
            Self::Opacity => "Opacity",
        }
    }

    /// Parameter definitions for this effect type.
    pub fn parameter_definitions(&self) -> Vec<ParameterDefinition> {
        match self {
            Self::Transform => vec![
                ParameterDefinition {
                    name: "x_offset".to_string(),
                    label: "X Offset".to_string(),
                    param_type: ParameterType::Float {
                        default: 0.0,
                        min: -10000.0,
                        max: 10000.0,
                    },
                },
                ParameterDefinition {
                    name: "y_offset".to_string(),
                    label: "Y Offset".to_string(),
                    param_type: ParameterType::Float {
                        default: 0.0,
                        min: -10000.0,
                        max: 10000.0,
                    },
                },
            ],
            Self::Grayscale => vec![],
            Self::Brightness => vec![ParameterDefinition {
                name: "brightness".to_string(),
                label: "Brightness".to_string(),
                param_type: ParameterType::Float {
                    default: 0.0,
                    min: -1.0,
                    max: 1.0,
                },
            }],
            Self::Opacity => vec![ParameterDefinition {
                name: "opacity".to_string(),
                label: "Opacity".to_string(),
                param_type: ParameterType::Float {
                    default: 1.0,
                    min: 0.0,
                    max: 1.0,
                },
            }],
        }
    }

    /// All built-in effect types.
    pub fn all_builtin() -> Vec<EffectType> {
        vec![
            EffectType::Transform,
            EffectType::Grayscale,
            EffectType::Brightness,
            EffectType::Opacity,
        ]
    }
}

/// The type of a parameter value.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ParameterType {
    Float { default: f64, min: f64, max: f64 },
}

/// Definition of a parameter on an effect.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ParameterDefinition {
    pub name: String,
    pub label: String,
    pub param_type: ParameterType,
}

/// A concrete parameter value.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ParameterValue {
    Float(f64),
}

// Manual Eq impl: f64 doesn't impl Eq, but we need this for Clip's Eq derive.
// We treat NaN == NaN which is fine for our use case (parameter values are always finite).
impl Eq for ParameterValue {}

/// An instance of an effect applied to a clip, with its parameter values.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EffectInstance {
    pub id: Uuid,
    pub effect_type: EffectType,
    pub parameters: Vec<(String, ParameterValue)>,
}

impl EffectInstance {
    /// Create a new effect instance with default parameter values.
    pub fn new(effect_type: EffectType) -> Self {
        let parameters = effect_type
            .parameter_definitions()
            .into_iter()
            .map(|def| {
                let value = match def.param_type {
                    ParameterType::Float { default, .. } => ParameterValue::Float(default),
                };
                (def.name, value)
            })
            .collect();
        Self {
            id: Uuid::new_v4(),
            effect_type,
            parameters,
        }
    }

    /// Get a float parameter value by name.
    pub fn get_float(&self, name: &str) -> Option<f64> {
        self.parameters.iter().find_map(|(n, v)| {
            if n == name {
                match v {
                    ParameterValue::Float(f) => Some(*f),
                }
            } else {
                None
            }
        })
    }

    /// Set a float parameter value by name. Returns true if found and set.
    pub fn set_float(&mut self, name: &str, value: f64) -> bool {
        for (n, v) in &mut self.parameters {
            if n == name {
                *v = ParameterValue::Float(value);
                return true;
            }
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_effect_instance_new_defaults() {
        let effect = EffectInstance::new(EffectType::Transform);
        assert_eq!(effect.effect_type, EffectType::Transform);
        assert_eq!(effect.get_float("x_offset"), Some(0.0));
        assert_eq!(effect.get_float("y_offset"), Some(0.0));
    }

    #[test]
    fn test_get_set_float_roundtrip() {
        let mut effect = EffectInstance::new(EffectType::Transform);
        assert!(effect.set_float("x_offset", 42.5));
        assert_eq!(effect.get_float("x_offset"), Some(42.5));
        assert!(effect.set_float("y_offset", -100.0));
        assert_eq!(effect.get_float("y_offset"), Some(-100.0));
    }

    #[test]
    fn test_get_float_nonexistent_param() {
        let effect = EffectInstance::new(EffectType::Transform);
        assert_eq!(effect.get_float("nonexistent"), None);
    }

    #[test]
    fn test_set_float_nonexistent_param() {
        let mut effect = EffectInstance::new(EffectType::Transform);
        assert!(!effect.set_float("nonexistent", 1.0));
    }

    #[test]
    fn test_serde_roundtrip() {
        let mut effect = EffectInstance::new(EffectType::Transform);
        effect.set_float("x_offset", 123.456);
        let json = serde_json::to_string(&effect).unwrap();
        let deserialized: EffectInstance = serde_json::from_str(&json).unwrap();
        assert_eq!(effect, deserialized);
    }

    #[test]
    fn test_effect_type_all_builtin() {
        let all = EffectType::all_builtin();
        assert_eq!(all.len(), 4);
        assert!(all.contains(&EffectType::Transform));
        assert!(all.contains(&EffectType::Grayscale));
        assert!(all.contains(&EffectType::Brightness));
        assert!(all.contains(&EffectType::Opacity));
    }

    #[test]
    fn test_effect_type_display_name() {
        assert_eq!(EffectType::Transform.display_name(), "Transform");
        assert_eq!(EffectType::Grayscale.display_name(), "Grayscale");
        assert_eq!(EffectType::Brightness.display_name(), "Brightness");
        assert_eq!(EffectType::Opacity.display_name(), "Opacity");
    }

    #[test]
    fn test_grayscale_no_params() {
        let effect = EffectInstance::new(EffectType::Grayscale);
        assert!(effect.parameters.is_empty());
    }

    #[test]
    fn test_brightness_default() {
        let effect = EffectInstance::new(EffectType::Brightness);
        assert_eq!(effect.get_float("brightness"), Some(0.0));
    }

    #[test]
    fn test_opacity_default() {
        let effect = EffectInstance::new(EffectType::Opacity);
        assert_eq!(effect.get_float("opacity"), Some(1.0));
    }

    #[test]
    fn test_serde_roundtrip_all_types() {
        for effect_type in EffectType::all_builtin() {
            let effect = EffectInstance::new(effect_type);
            let json = serde_json::to_string(&effect).unwrap();
            let deserialized: EffectInstance = serde_json::from_str(&json).unwrap();
            assert_eq!(effect, deserialized);
        }
    }
}
