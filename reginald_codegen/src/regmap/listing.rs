use crate::{
    error::Error,
    regmap::{TypeAdr, TypeBitwidth, TypeValue},
};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, io};

// ==== Basic Types ============================================================

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq)]
#[serde(untagged, deny_unknown_fields)]
pub enum BitRange {
    Bit(TypeBitwidth),
    Range(String),
}

pub type Bits = Vec<BitRange>;

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq)]
pub enum AccessMode {
    R,
    W,
}

pub type Access = Vec<AccessMode>;

// ==== Enums ==================================================================

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct EnumEntry {
    pub val: TypeValue,
    pub doc: Option<String>,
}

pub type EnumEntries = BTreeMap<String, EnumEntry>;

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SharedEnum {
    pub doc: Option<String>,
    #[serde(rename = "enum")]
    pub entries: EnumEntries,
}

// ==== Layouts ================================================================

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
#[derive(Default)]
pub enum FieldType {
    #[default]
    UInt,
    Bool,
    Fixed(TypeValue),
    Enum(EnumEntries),
    SharedEnum(String),
    Layout(LayoutFields),
    SharedLayout(String),
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq, Default)]
#[serde(deny_unknown_fields)]
pub struct LayoutField {
    pub bits: Bits,
    pub doc: Option<String>,
    #[serde(default)]
    pub accepts: FieldType,
    pub access: Option<Access>,
}

pub type LayoutFields = BTreeMap<String, LayoutField>;

// TODO: Implement custom deser logic to allow untagged representation?
#[derive(Serialize, Deserialize, Debug, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub enum RegisterLayout {
    Layout(LayoutFields),
    SharedLayout(String),
}

impl Default for RegisterLayout {
    fn default() -> Self {
        Self::Layout(BTreeMap::new())
    }
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SharedLayout {
    pub doc: Option<String>,
    pub bitwidth: Option<TypeBitwidth>,
    pub layout: LayoutFields,
}

// ==== Individual Register ====================================================

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq, Default)]
#[serde(deny_unknown_fields)]
pub struct Register {
    pub adr: TypeAdr,
    pub doc: Option<String>,

    pub bitwidth: Option<TypeBitwidth>,
    pub reset_val: Option<TypeValue>,

    #[serde(default)]
    pub layout: RegisterLayout,
}

// ==== Register Block =========================================================

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq, Default)]
#[serde(deny_unknown_fields)]
pub struct Instance {
    pub adr: TypeAdr,

    pub doc: Option<String>,

    #[serde(default = "BTreeMap::new")]
    pub reset_vals: BTreeMap<String, TypeValue>,
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq, Default)]
#[serde(deny_unknown_fields)]
pub struct RegisterBlockMember {
    pub offset: TypeAdr,
    pub doc: Option<String>,

    pub bitwidth: Option<TypeBitwidth>,
    pub reset_val: Option<TypeValue>,

    pub layout: RegisterLayout,
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RegisterBlock {
    pub instances: BTreeMap<String, Instance>,
    pub doc: Option<String>,

    pub registers: BTreeMap<String, RegisterBlockMember>,
}

// ==== Register Map ===========================================================

// TODO: Implement custom deser logic to allow untagged representation?
#[derive(Serialize, Deserialize, Debug, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub enum RegisterListing {
    Register(Register),
    RegisterBlock(RegisterBlock),
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq, Default)]
#[serde(deny_unknown_fields)]
pub struct Defaults {
    pub layout_bitwidth: Option<TypeBitwidth>,
    pub field_access_mode: Option<Access>,
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq, Default)]
#[serde(deny_unknown_fields)]
pub struct RegisterMap {
    pub name: String,
    pub doc: Option<String>,
    pub notice: Option<String>,
    pub author: Option<String>,

    #[serde(default)]
    pub defaults: Defaults,

    #[serde(default = "BTreeMap::new")]
    pub enums: BTreeMap<String, SharedEnum>,

    #[serde(default = "BTreeMap::new")]
    pub layouts: BTreeMap<String, SharedLayout>,

    #[serde(default = "BTreeMap::new")]
    pub registers: BTreeMap<String, RegisterListing>,
}

impl RegisterMap {
    pub fn from_yaml<R>(inp: R) -> Result<Self, Error>
    where
        R: io::Read,
    {
        Ok(serde_yaml::from_reader(inp)?)
    }

    pub fn from_hjson<R>(inp: R) -> Result<Self, Error>
    where
        R: io::Read,
    {
        Ok(deser_hjson::from_reader(inp)?)
    }
}

// ==== Tests ==================================================================

#[cfg(test)]
mod tests {
    use super::*;

    use std::path::PathBuf;

    use deser_hjson;
    use lazy_static::lazy_static;
    use pretty_assertions::assert_eq;
    use serde_yaml;

    #[test]
    fn deser_yaml_bits() {
        let yaml = "[\"2-3\"]";
        let v: Bits = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(v, vec![BitRange::Range("2-3".into())]);

        let yaml = "[2]";
        let v: Bits = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(v, vec![BitRange::Bit(2)]);

        let yaml = "[1-3, 4, \"5-6\"]";
        let v: Bits = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(
            v,
            vec![
                BitRange::Range("1-3".into()),
                BitRange::Bit(4),
                BitRange::Range("5-6".into())
            ]
        );
    }

    #[test]
    fn deser_hjson_bits() {
        let hjson = "[\"2-3\"]";
        let v: Bits = deser_hjson::from_str(hjson).unwrap();
        assert_eq!(v, vec![BitRange::Range("2-3".into())]);

        let hjson = "[2]";
        let v: Bits = deser_hjson::from_str(hjson).unwrap();
        assert_eq!(v, vec![BitRange::Bit(2)]);

        let hjson = "[\"1-3\", 4, \"5-6\"]";
        let v: Bits = deser_hjson::from_str(hjson).unwrap();
        assert_eq!(
            v,
            vec![
                BitRange::Range("1-3".into()),
                BitRange::Bit(4),
                BitRange::Range("5-6".into())
            ]
        );
    }

    #[test]
    fn deser_yaml_access() {
        let yaml = "['R']";
        let v: Access = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(v, vec![AccessMode::R]);

        let yaml = "[W]";
        let v: Access = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(v, vec![AccessMode::W]);

        let yaml = "['W', R]";
        let v: Access = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(v, vec![AccessMode::W, AccessMode::R]);
    }

    #[test]
    fn deser_hjson_access() {
        let hjson = "['R']";
        let v: Access = deser_hjson::from_str(hjson).unwrap();
        assert_eq!(v, vec![AccessMode::R]);

        let hjson = "[W]";
        let v: Access = deser_hjson::from_str(hjson).unwrap();
        assert_eq!(v, vec![AccessMode::W]);

        let hjson = "['W', R]";
        let v: Access = deser_hjson::from_str(hjson).unwrap();
        assert_eq!(v, vec![AccessMode::W, AccessMode::R]);
    }

    #[test]
    fn deser_yaml_empty_map() {
        let yaml = "
        name: DummyChip
        ";
        let is: RegisterMap = serde_yaml::from_str(yaml).unwrap();
        let expect = RegisterMap {
            name: "DummyChip".to_string(),
            ..Default::default()
        };
        assert_eq!(is, expect);
    }

    #[test]
    fn deser_hjson_empty_map() {
        let hjson = "
        name: DummyChip
        ";
        let is: RegisterMap = deser_hjson::from_str(hjson).unwrap();
        let expect = RegisterMap {
            name: "DummyChip".to_string(),
            ..Default::default()
        };
        assert_eq!(is, expect);
    }

    lazy_static! {
        static ref SHARED_ENUM_EXPECT: RegisterMap = RegisterMap {
            name: "DummyChip".to_string(),
            enums: BTreeMap::from([(
                "MyEnum".into(),
                SharedEnum {
                    doc: None,
                    entries: BTreeMap::from([("OFF".into(), EnumEntry { val: 0x0, doc: None },)]),
                },
            )]),
            ..Default::default()
        };
    }

    #[test]
    fn deser_yaml_shared_enums() {
        let yaml = "
        name: DummyChip
        enums:
            MyEnum:
                enum:
                    OFF:
                        val: 0
        ";
        let is: RegisterMap = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(is, *SHARED_ENUM_EXPECT);
    }

    #[test]
    fn deser_hjson_shared_enums() {
        let hjson = "
        name: DummyChip
        enums: {
            MyEnum: {
                enum: {
                    OFF: {
                        val: 0
                    }
                }
            }
        }
        ";
        let is: RegisterMap = deser_hjson::from_str(hjson).unwrap();
        assert_eq!(is, *SHARED_ENUM_EXPECT);
    }

    lazy_static! {
        static ref BASIC_REGISTER_EXPECT: RegisterMap = RegisterMap {
            name: "DummyChip".to_string(),
            registers: BTreeMap::from([(
                "FIFOCTRL4".into(),
                RegisterListing::Register(Register {
                    adr: 0x10,
                    layout: RegisterLayout::Layout(BTreeMap::from([
                        (
                            "F7".into(),
                            LayoutField {
                                bits: vec![BitRange::Bit(7)],
                                ..Default::default()
                            },
                        ),
                        (
                            "F1".into(),
                            LayoutField {
                                bits: vec![BitRange::Bit(1)],
                                ..Default::default()
                            },
                        ),
                    ])),
                    ..Default::default()
                }),
            )]),
            ..Default::default()
        };
    }

    #[test]
    fn deser_yaml_basic_register() {
        let yaml = "
        name: DummyChip
        registers:
            FIFOCTRL4: !Register
                adr: 0x10
                layout: !Layout
                    F7:
                        bits: [7]
                    F1:
                        bits: [1]
        ";
        let is: RegisterMap = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(is, *BASIC_REGISTER_EXPECT);
    }

    #[test]
    fn deser_hjson_basic_register() {
        let hjson = "
        name: DummyChip
        registers: {
            FIFOCTRL4: {
                Register: {
                    adr: 16,
                    layout: {
                        Layout: {
                            F7: {
                                bits: [7]
                            },
                            F1: {
                                bits: [1]
                            }
                        }
                    }
                }
            }
        }
        ";
        let is: RegisterMap = deser_hjson::from_str(hjson).unwrap();
        assert_eq!(is, *BASIC_REGISTER_EXPECT);
    }

    lazy_static! {
        static ref FIELD_ENUM_EXCEPT: LayoutField = LayoutField {
            bits: vec![BitRange::Bit(1)],
            doc: None,
            accepts: FieldType::Enum(BTreeMap::from([
                ("A".into(), EnumEntry { val: 0x1, doc: None },),
                ("B".into(), EnumEntry { val: 0x0, doc: None },),
            ])),
            access: None,
        };
    }

    #[test]
    fn deser_yaml_field_enum() {
        let yaml = "
        bits: [1]
        accepts: !Enum
            A:
                val: 0x1
            B:
                val: 0x0
        ";
        let field_is: LayoutField = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(field_is, *FIELD_ENUM_EXCEPT);
    }

    #[test]
    fn deser_hjson_field_enum() {
        let hjson = "
        bits: [1]
        accepts: {
            Enum: {
                A: {
                    val: 1
                },
                B: {
                    val: 0
                },
            }
        }
        ";
        let field_is: LayoutField = deser_hjson::from_str(hjson).unwrap();
        assert_eq!(field_is, *FIELD_ENUM_EXCEPT);
    }

    lazy_static! {
        static ref FIELD_SHARED_ENUM_EXPECT: LayoutField = LayoutField {
            bits: vec![BitRange::Bit(1)],
            doc: None,
            accepts: FieldType::SharedEnum("TestEnum".into()),
            access: None,
        };
    }

    #[test]
    fn deser_yaml_field_shared_enum() {
        let yaml = "
        bits: [1]
        accepts: !SharedEnum 'TestEnum'
        ";
        let field_is: LayoutField = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(field_is, *FIELD_SHARED_ENUM_EXPECT);
    }

    #[test]
    fn deser_hjson_field_shared_enum() {
        let hjson = "
        bits: [1]
        accepts: {
            SharedEnum: 'TestEnum'
        }
        ";
        let field_is: LayoutField = deser_hjson::from_str(hjson).unwrap();
        assert_eq!(field_is, *FIELD_SHARED_ENUM_EXPECT);
    }

    fn parse_yaml_example(file: &str) -> RegisterMap {
        let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        path.push("../examples/maps/");
        path.push(file);
        let reader = std::fs::File::open(path).unwrap();
        RegisterMap::from_yaml(reader).unwrap()
    }

    fn parse_hjson_example(file: &str) -> RegisterMap {
        let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        path.push("../examples/maps/");
        path.push(file);
        let reader = std::fs::File::open(path).unwrap();
        RegisterMap::from_hjson(reader).unwrap()
    }

    #[test]
    fn deser_example_dummy_yaml() {
        parse_yaml_example("dummy.yaml");
    }

    #[test]
    fn deser_example_dummy_hjson() {
        parse_hjson_example("dummy.hjson");
    }

    #[test]
    fn deser_example_max77654_yaml() {
        parse_yaml_example("max77654.yaml");
    }

    #[test]
    fn deser_example_max77654_hjson() {
        parse_hjson_example("max77654.hjson");
    }
}
