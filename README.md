# Reginald

Philipp Schilk
2022-2024

### TODOs:

- TEST NO-CONTINOUS FIELDS!
    - With enum!

- TEST VALIDATION: FIELD MUCH LARGER THAN ENUM

- TEST: OVERVWRITE PRESERVES? (Funcpack)
    - With enum!


- No limit on max reg size?
    - YAML/Json limits -> Allow int & string in 'type value' fields?
    - What 'bigint' crate?
        - Probably rework convert/regmap + generators first?


- c.funcpack: just include both LE and BE in the same generator?
- c.funcpack: Add uint-to-bytes wrapper?
- c.funcpack: Remove 'overwrite'. Add general option for stuffing unused bytes with 'reserved' fields?

- Allow fields that are registers? ?!
- RS uint generator?

- change c-funcpack field_enum_prefix default but issue warning?

### RUST API NOTES:

bitfield-struct:

```rust
#[bitfield(u64)]
#[derive(PartialEq, Eq)] // <- Attributes after `bitfield` are carried over
struct MyBitfield {
    /// Defaults to 16 bits for u16
    int: u16,
    /// Interpreted as 1 bit flag, with a custom default value
    #[bits(default = true)]
    flag: bool,
    /// Custom bit size
    #[bits(1)]
    tiny: u8,
    /// Sign extend for signed integers
    #[bits(13)]
    negative: i16,
    /// Supports any type with `into_bits`/`from_bits` functions
    #[bits(16)]
    custom: CustomEnum,
    /// Public field -> public accessor functions
    #[bits(10)]
    pub public: usize,
    /// Also supports read-only fields
    #[bits(1, access = RO)]
    read_only: bool,
    /// And write-only fields
    #[bits(1, access = WO)]
    write_only: bool,
    /// Padding
    #[bits(5)]
    __: u8,
}

let raw: u64 = val.into();
```

https://github.com/ProfFan/dw3000-ng/blob/RUST/src/ll.rs
https://github.com/jkelleyrtp/dw1000-rs
rust embedded matrix

# Packed structs libs:

### Packed Struct:
    - https://docs.rs/packed_struct/latest/packed_struct/
    - Strange API? What does read/write only do?

### Bitfield struct:
    - https://crates.io/crates/bitfield-struct
