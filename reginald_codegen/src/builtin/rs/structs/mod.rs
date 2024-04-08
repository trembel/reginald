use std::{collections::HashSet, fmt::Write};

use crate::{
    bits::{bitmask_from_width, lsb_pos, mask_to_bit_ranges_str, mask_width, msb_pos, unpositioned_mask},
    builtin::rs::rs_const,
    error::Error,
    regmap::{
        Enum, FieldType, Layout, LayoutField, Register, RegisterBlock, RegisterBlockMember, RegisterMap, TypeBitwidth,
        TypeValue,
    },
    utils::{
        field_byte_to_packed_byte_transform, field_to_packed_byte_transform, filename, grab_byte,
        packed_byte_to_field_byte_transform, packed_byte_to_field_transform, remove_wrapping_parens, Endianess,
        ShiftDirection,
    },
    writer::{header_writer::HeaderWriter, indent_writer::IndentWriter},
};
use clap::Parser;

use super::{
    generate_doc_comment, rs_fitting_unsigned_type, rs_generate_header_comment, rs_generate_section_header_comment,
    rs_header_comment, rs_layout_overview_comment, rs_pascalcase, rs_section_header_comment, rs_snakecase,
};

// ====== Generator Opts =======================================================

#[derive(Debug, Clone)]
#[cfg_attr(feature = "cli", derive(Parser))]
pub struct GeneratorOpts {
    /// Rust type to use for register addresses.
    ///
    /// If none is specified, the smallest unsigned type capable of storing
    /// the largest address will be used.
    #[cfg_attr(feature = "cli", arg(long))]
    #[cfg_attr(feature = "cli", arg(action = clap::ArgAction::Set))]
    #[cfg_attr(feature = "cli", arg(verbatim_doc_comment))]
    pub address_type: Option<String>,

    /// Include static string error messages for unpacking errors.
    #[cfg_attr(feature = "cli", arg(long))]
    #[cfg_attr(feature = "cli", arg(action = clap::ArgAction::Set))]
    #[cfg_attr(feature = "cli", arg(default_value = "false"))]
    #[cfg_attr(feature = "cli", arg(verbatim_doc_comment))]
    pub unpacking_error_msg: bool,

    /// Trait to derive on all register structs.
    ///
    /// May be given multiple times.
    #[cfg_attr(feature = "cli", arg(long))]
    #[cfg_attr(feature = "cli", arg(action = clap::ArgAction::Append))]
    #[cfg_attr(feature = "cli", arg(verbatim_doc_comment))]
    pub struct_derive: Vec<String>,

    /// Trait to derive on all enums.
    ///
    /// May be given multiple times. Note: All enums always derive
    /// the "Clone" and "Copy" traits.
    #[cfg_attr(feature = "cli", arg(long = "enum-derive"))]
    #[cfg_attr(feature = "cli", arg(value_name = "DERIVE"))]
    #[cfg_attr(feature = "cli", arg(action = clap::ArgAction::Append))]
    #[cfg_attr(feature = "cli", arg(verbatim_doc_comment))]
    pub raw_enum_derive: Vec<String>,

    /// Module should be 'use'ed at the top of the generated module.
    ///
    /// May be given multiple times.
    #[cfg_attr(feature = "cli", arg(long))]
    #[cfg_attr(feature = "cli", arg(action = clap::ArgAction::Append))]
    #[cfg_attr(feature = "cli", arg(verbatim_doc_comment))]
    pub add_use: Vec<String>,

    /// Use an external definition of the `ToBytes`/`FromBytes`/`TryFromBytes` traits,
    ///
    /// No trait definition are generated, and implementations of the traits refeer
    /// to `[prefix]ToBytes`, `[prefix]FromBytes`, and `[prefix]TryFromBytes`,
    /// where `[preifx]` is the value given to this flag.
    #[cfg_attr(feature = "cli", arg(long))]
    #[cfg_attr(feature = "cli", arg(action = clap::ArgAction::Set))]
    #[cfg_attr(feature = "cli", arg(verbatim_doc_comment))]
    pub external_traits: Option<String>,

    /// Generate `Default` implementations for all register structs, using the reset
    /// value - if given.
    #[cfg_attr(feature = "cli", arg(long))]
    #[cfg_attr(feature = "cli", arg(action = clap::ArgAction::Set))]
    #[cfg_attr(feature = "cli", arg(default_value = "true"))]
    #[cfg_attr(feature = "cli", arg(verbatim_doc_comment))]
    pub generate_defaults: bool,

    /// Generate `From/TryFrom/From` implementations that convert a register
    /// to/from the smallest rust unsigned integer value wide enough to hold the
    /// register, if one exists.
    #[cfg_attr(feature = "cli", arg(long))]
    #[cfg_attr(feature = "cli", arg(action = clap::ArgAction::Set))]
    #[cfg_attr(feature = "cli", arg(default_value = "true"))]
    #[cfg_attr(feature = "cli", arg(verbatim_doc_comment))]
    pub generate_uint_conversion: bool,
}

// ====== Generator ============================================================

pub fn generate(out: &mut dyn Write, map: &RegisterMap, opts: &GeneratorOpts) -> Result<(), Error> {
    // Determine address type: Use option override, or smallest
    // unsigned type that fits the largest address in the map.
    let address_type = if let Some(address_type) = &opts.address_type {
        address_type.clone()
    } else {
        let max_addr = map.registers.values().map(|x| x.adr).max().unwrap_or(0);
        rs_fitting_unsigned_type(msb_pos(max_addr) + 1)?
    };

    // Determine all enums that, while not fulling coverng an n-bit value,
    // are used in non-continous fields that can only take valus that
    // the enum can represent. These enums require 'truncating conversion'
    // function.
    let mut enums_requiring_truncating_conv: HashSet<String> = HashSet::new();
    for layout in map.layouts.values() {
        for field in layout.fields.values() {
            if let FieldType::Enum(field_enum) = &field.accepts {
                if field.can_always_unpack()
                    && !field_enum.can_unpack_min_bitwidth()
                    && field_enum.can_do_truncating_unpacking()
                {
                    enums_requiring_truncating_conv.insert(field_enum.name.clone());
                }
            };
        }
    }

    let mut enum_derives: Vec<String> = vec!["Clone".into(), "Copy".into()];
    enum_derives.extend(opts.raw_enum_derive.clone());

    // Generate
    let generator = Generator {
        opts: opts.clone(),
        enum_derives,
        address_type,
        map,
        enums_requiring_truncating_conv,
    };
    generator.generate(out)?;
    Ok(())
}

struct Generator<'a> {
    opts: GeneratorOpts,
    map: &'a RegisterMap,
    address_type: String,
    enum_derives: Vec<String>,
    enums_requiring_truncating_conv: HashSet<String>,
}

impl Generator<'_> {
    /// Generate complete file
    fn generate(&self, out: &mut dyn Write) -> Result<(), Error> {
        let mut out = HeaderWriter::new(out);

        // File header/preamble:
        self.generate_header(&mut out)?;

        if self.opts.external_traits.is_none() {
            self.generate_traits(&mut out)?;
        }

        // ===== Shared enums: =====

        out.push_section_with_header(&["\n", &rs_section_header_comment("Shared Enums"), "\n"]);

        for shared_enum in self.map.shared_enums() {
            out.push_section_with_header(&["\n", &rs_header_comment(&shared_enum.name), "\n"]);
            self.generate_enum(&mut out, shared_enum)?;
            out.pop_section();
        }

        out.pop_section();

        // ===== Shared layouts: =====

        out.push_section_with_header(&["\n", &rs_section_header_comment("Shared Layout Structs"), "\n"]);
        for layout in self.map.shared_layouts() {
            out.push_section_with_header(&["\n", &rs_header_comment(&layout.name), "\n"]);
            self.generate_layout(&mut out, layout, true)?;
            out.pop_section();
        }
        out.pop_section();

        // ===== Individual Registers: =====
        for register in self.map.individual_registers() {
            let mut header = String::new();
            Self::generate_register_header(&mut header, register)?;
            out.push_section_with_header(&[&header]);

            self.generate_register_properties(&mut out, register)?;

            // If the layout is local to this register, generate it:
            if register.layout.is_local {
                self.generate_layout(&mut out, &register.layout, true)?;
            } else {
                writeln!(&mut out)?;
                writeln!(out, "// Register uses the {} struct.", rs_pascalcase(&register.layout.name))?;
            }

            out.pop_section();
        }

        // ===== Register Blocks: =====

        for block in self.map.register_blocks.values() {
            let mut header = String::new();
            Self::generate_register_block_header(&mut header, block)?;
            out.push_section_with_header(&[&header]);

            self.generate_register_block_properties(&mut out, block)?;

            for member in block.members.values() {
                let mut header = String::new();
                Self::generate_register_block_member_header(&mut header, member)?;
                out.push_section_with_header(&[&header]);

                self.generate_register_block_member_properties(&mut out, block, member)?;

                if member.layout.is_local {
                    self.generate_layout(&mut out, &member.layout, true)?;
                } else {
                    writeln!(&mut out)?;
                    writeln!(out, "// Register uses the {} struct.", rs_pascalcase(&member.layout.name))?;
                }

                out.pop_section();
            }

            out.pop_section();
        }

        Ok(())
    }

    /// Generate file header
    fn generate_header(&self, out: &mut dyn Write) -> Result<(), Error> {
        writeln!(out, "#![allow(clippy::unnecessary_cast)]")?;

        // Top doc comment:
        writeln!(out, "//! `{}` Registers", self.map.name)?;
        writeln!(out, "//!")?;

        // Generated-with-reginald note, including original file name if known:
        if let Some(input_file) = &self.map.from_file {
            writeln!(out, "//! Generated using reginald from `{}`.", filename(input_file)?)?;
        } else {
            writeln!(out, "//! Generated using reginald.")?;
        }

        // Indicate which generator was used:
        writeln!(out, "//! Generator: rs-structs")?;

        // Map top-level documentation:
        if !self.map.docs.is_empty() {
            writeln!(out, "//!")?;
            write!(out, "{}", self.map.docs.as_multiline("//! "))?;
        }

        // Map author and note:
        if let Some(author) = &self.map.author {
            writeln!(out, "//! ")?;
            writeln!(out, "//! Listing file author: {author}")?;
        }
        if let Some(notice) = &self.map.notice {
            writeln!(out, "//!")?;
            writeln!(out, "//! Listing file notice:")?;
            for line in notice.lines() {
                writeln!(out, "//!   {line}")?;
            }
        }

        if !self.opts.add_use.is_empty() {
            writeln!(out)?;
            for add_use in &self.opts.add_use {
                writeln!(out, "use {add_use};")?;
            }
        }

        Ok(())
    }

    const CONVERSION_TRAITS: &'static str = include_str!("traits.txt");

    fn generate_traits(&self, out: &mut dyn Write) -> Result<(), Error> {
        writeln!(out)?;
        rs_generate_section_header_comment(out, "Traits")?;
        writeln!(out)?;
        write!(out, "{}", Self::CONVERSION_TRAITS)?;
        Ok(())
    }

    /// Generate enum
    fn generate_enum(&self, out: &mut dyn Write, e: &Enum) -> Result<(), Error> {
        // Smallest uint type that can be used to represent the enum's content:
        let uint_type = rs_fitting_unsigned_type(e.min_bitdwith())?;

        writeln!(out)?;
        generate_doc_comment(out, &e.docs, "")?;

        // Enum derives:
        let derives = self.enum_derives.join(", ");
        writeln!(out, "#[derive({derives})]")?;

        // Enum proper:
        writeln!(out, "#[repr({uint_type})]")?;
        writeln!(out, "pub enum {} {{", rs_pascalcase(&e.name))?;
        for entry in e.entries.values() {
            generate_doc_comment(out, &entry.docs, "    ")?;
            writeln!(out, "    {} = 0x{:x},", rs_pascalcase(&entry.name), entry.value)?;
        }
        writeln!(out, "}}")?;

        // Enum impl for uint -> enum conversion:
        self.generate_enum_impl(out, e)?;

        Ok(())
    }

    fn generate_enum_impl(&self, out: &mut dyn Write, e: &Enum) -> Result<(), Error> {
        // Smallest uint type that can be used to represent the enum's content:
        let uint_type = rs_fitting_unsigned_type(e.min_bitdwith())?;

        let enum_name = rs_pascalcase(&e.name);

        if e.can_unpack_min_bitwidth() {
            // If the enum can represent every value that fits into a N-bit value, where
            // N is its minimal bitwidth, implement a 'Try' wrapping conversion:
            writeln!(out)?;
            writeln!(out, "impl From<{uint_type}> for {enum_name} {{")?;
            writeln!(out, "    fn from(value: {uint_type}) -> Self {{")?;
            writeln!(out, "        match value & 0x{:X} {{", bitmask_from_width(e.min_bitdwith()))?;
            for entry in e.entries.values() {
                writeln!(out, "            0x{:X} => Self::{},", entry.value, rs_pascalcase(&entry.name))?;
            }
            writeln!(out, "            _ => unreachable!(),")?;
            writeln!(out, "        }}")?;
            writeln!(out, "    }}")?;
            writeln!(out, "}}")?;
        } else {
            // Otherwise, implement a try-from conversion:
            writeln!(out)?;
            writeln!(out, "impl TryFrom<{uint_type}> for {enum_name} {{")?;

            // Error type:
            if self.opts.unpacking_error_msg {
                writeln!(out, "    type Error = &'static str;")?;
            } else {
                writeln!(out, "    type Error = ();")?;
            }

            // Conversion:
            writeln!(out)?;
            writeln!(out, "    fn try_from(value: {uint_type}) -> Result<Self, Self::Error> {{")?;
            writeln!(out, "        match value {{")?;
            for entry in e.entries.values() {
                writeln!(out, "            0x{:X} => Ok(Self::{}),", entry.value, rs_pascalcase(&entry.name))?;
            }
            if self.opts.unpacking_error_msg {
                writeln!(out, "            _ => Err(\"{} unpack error\"),", rs_pascalcase(&e.name))?;
            } else {
                writeln!(out, "            _ => Err(()),")?;
            }
            writeln!(out, "        }}")?;
            writeln!(out, "    }}")?;

            writeln!(out, "}}")?;
        }

        if self.enums_requiring_truncating_conv.contains(&e.name) {
            writeln!(out)?;
            writeln!(out, "impl {enum_name} {{")?;
            writeln!(out, "    pub fn truncated_from(value: {uint_type}) -> Self {{")?;
            writeln!(out, "        match value & 0x{:X} {{", e.occupied_bits())?;
            for entry in e.entries.values() {
                writeln!(out, "            0x{:X} => Self::{},", entry.value, rs_pascalcase(&entry.name))?;
            }
            writeln!(out, "            _ => unreachable!(),")?;
            writeln!(out, "        }}")?;
            writeln!(out, "    }}")?;
            writeln!(out, "}}")?;
        }

        Ok(())
    }

    fn generate_layout(&self, out: &mut dyn Write, layout: &Layout, generate_headers: bool) -> Result<(), Error> {
        let mut out = HeaderWriter::new(out);

        if generate_headers {
            if layout.is_local {
                out.push_section_with_header(&["\n", "// Register-specific enums and sub-layouts:", "\n"]);
            } else {
                out.push_section_with_header(&["\n", "// Layout-specific enums and sub-layouts:", "\n"]);
            }
        }

        for e in layout.local_enums() {
            self.generate_enum(&mut out, e)?;
        }

        for local_layout in layout.local_layouts() {
            self.generate_layout(&mut out, local_layout, false)?;
        }

        if generate_headers {
            out.pop_section();
        }

        if generate_headers {
            if layout.is_local {
                out.push_section_with_header(&["\n", "// Register Layout Struct:", "\n"]);
            } else {
                out.push_section_with_header(&["\n", "// Layout Struct:", "\n"]);
            }
        }

        self.generate_layout_struct(&mut out, layout)?;

        if generate_headers {
            out.pop_section();
        }

        if generate_headers {
            out.push_section_with_header(&["\n", "// Struct Conversion Functions:", "\n"]);
        }

        self.generate_layout_impl_to_bytes(&mut out, layout)?;
        self.generate_layout_impl_from_bytes(&mut out, layout)?;
        if self.opts.generate_uint_conversion {
            self.generate_layout_impl_uint_conv(&mut out, layout)?;
        }

        if generate_headers {
            out.pop_section();
        }
        Ok(())
    }

    fn generate_layout_struct(&self, out: &mut dyn Write, layout: &Layout) -> Result<(), Error> {
        // Struct doc comment:
        writeln!(out)?;
        writeln!(out, "/// `{}`", layout.name)?;
        if !layout.docs.is_empty() {
            writeln!(out, "///")?;
            write!(out, "{}", layout.docs.as_multiline("/// "))?;
        }

        // Register derives:
        if !self.opts.struct_derive.is_empty() {
            let derives = self.opts.struct_derive.join(", ");
            writeln!(out, "#[derive({derives})]")?;
        }

        // Struct proper:
        writeln!(out, "pub struct {} {{", rs_pascalcase(&layout.name))?;

        for field in layout.fields_with_content() {
            let field_type = self.register_layout_member_type(field)?;
            let field_name = rs_snakecase(&field.name);
            generate_doc_comment(out, &field.docs, "    ")?;
            writeln!(out, "    pub {field_name}: {field_type},")?;
        }

        writeln!(out, "}}")?;

        Ok(())
    }

    /// Type of a field inside a register struct.
    fn register_layout_member_type(&self, field: &LayoutField) -> Result<String, Error> {
        match &field.accepts {
            FieldType::Enum(e) => Ok(rs_pascalcase(&e.name)),
            FieldType::UInt => rs_fitting_unsigned_type(mask_width(field.mask)),
            FieldType::Bool => Ok("bool".to_string()),
            FieldType::Layout(layout) => Ok(rs_pascalcase(&layout.name)),
            FieldType::Fixed(_) => panic!("Fixed layout field has no type"),
        }
    }

    fn generate_layout_impl_to_bytes(&self, out: &mut dyn Write, layout: &Layout) -> Result<(), Error> {
        let struct_name = rs_pascalcase(&layout.name);
        let width_bytes = layout.width_bytes();
        let trait_prefix = self.trait_prefix();

        let mut out = IndentWriter::new(out, "    ");

        // Impl block and function signature:
        writeln!(out)?;
        writeln!(out, "impl {trait_prefix}ToBytes<{width_bytes}> for {struct_name} {{")?;
        writeln!(out, "    #[allow(clippy::cast_possible_truncation)]")?;
        writeln!(out, "    fn to_le_bytes(&self) -> [u8; {width_bytes}] {{")?;

        if layout.fields.is_empty() {
            writeln!(out, "        [0; {width_bytes}]")?;
            writeln!(out, "    }}")?;
            writeln!(out, "}}")?;
            return Ok(());
        }

        out.increase_indent(2);

        // Variable to hold result:
        writeln!(out, "let mut val: [u8; {width_bytes}] = [0; {width_bytes}];")?;

        // Insert each field:
        for field in layout.fields.values() {
            let field_name = rs_snakecase(&field.name);

            writeln!(out, "// {} @ {struct_name}[{}]:", field.name, mask_to_bit_ranges_str(field.mask))?;

            match &field.accepts {
                FieldType::UInt | FieldType::Bool | FieldType::Enum(_) => {
                    // Numeric field that can be directly converted:
                    for byte in 0..width_bytes {
                        let Some(transform) = field_to_packed_byte_transform(
                            Endianess::Little,
                            unpositioned_mask(field.mask),
                            lsb_pos(field.mask),
                            byte,
                            width_bytes,
                        ) else {
                            continue;
                        };

                        // Convert the field to some unsigned integer that can be shifted:
                        let field_value = match &field.accepts {
                            FieldType::UInt => format!("self.{field_name}"),
                            FieldType::Bool => format!("u8::from(self.{field_name})"),
                            FieldType::Enum(e) => {
                                let enum_uint = rs_fitting_unsigned_type(e.min_bitdwith())?;
                                format!("(self.{field_name} as {enum_uint})")
                            }
                            FieldType::Fixed(_) => unreachable!(),
                            FieldType::Layout(_) => unreachable!(),
                        };

                        // The byte of interest:
                        let field_byte = match &transform.shift {
                            Some((ShiftDirection::Left, amnt)) => format!("({field_value} << {amnt})"),
                            Some((ShiftDirection::Right, amnt)) => format!("({field_value} >> {amnt})"),
                            None => field_value,
                        };

                        let masked_field_byte = if transform.mask == 0xFF {
                            field_byte
                        } else {
                            format!("({field_byte} & 0x{:X})", transform.mask)
                        };

                        writeln!(out, "val[{byte}] |= {masked_field_byte} as u8;")?;
                    }
                }

                FieldType::Fixed(fixed) => {
                    // Fixed value:
                    for byte in 0..width_bytes {
                        let mask_byte = grab_byte(Endianess::Little, field.mask, byte, width_bytes);
                        let value_byte = grab_byte(Endianess::Little, *fixed << lsb_pos(field.mask), byte, width_bytes);
                        if mask_byte == 0 {
                            continue;
                        };

                        writeln!(out, "val[{byte}] |= 0x{value_byte:x}; // Fixed value.")?;
                    }
                }

                FieldType::Layout(sublayout) => {
                    // Sub-layout has to delegate to other pack function:
                    let array_name = rs_snakecase(&field.name);
                    let array_len = sublayout.width_bytes();

                    if sublayout.fields.is_empty() {
                        writeln!(out, "// No fields.")?;
                        continue;
                    }

                    writeln!(out, "let {array_name}: [u8; {array_len}] = self.{field_name}.to_le_bytes();")?;

                    for byte in 0..width_bytes {
                        for field_byte in 0..array_len {
                            // Determine required transform to put byte 'field_byte' of field into 'byte' of
                            // output:
                            let transform = field_byte_to_packed_byte_transform(
                                Endianess::Little,
                                sublayout.occupied_mask(),
                                lsb_pos(field.mask),
                                field_byte,
                                sublayout.width_bytes(),
                                byte,
                                width_bytes,
                            );

                            let Some(transform) = transform else {
                                continue;
                            };

                            let field_byte = format!("{array_name}[{field_byte}]");
                            let field_byte = match &transform.shift {
                                Some((ShiftDirection::Left, amnt)) => format!("({field_byte} << {amnt})"),
                                Some((ShiftDirection::Right, amnt)) => format!("({field_byte} >> {amnt})"),
                                None => field_byte,
                            };

                            let masked = if transform.mask != 0xFF {
                                format!("{field_byte} & 0x{:X}", transform.mask)
                            } else {
                                field_byte
                            };

                            writeln!(out, "val[{byte}] |= {masked};")?;
                        }
                    }
                }
            }
        }

        // Return result:
        writeln!(out, "val")?;

        // End of impl block/signature:
        out.decrease_indent(2);
        writeln!(out, "    }}")?;
        writeln!(out, "}}")?;

        Ok(())
    }

    fn generate_layout_impl_from_bytes(&self, out: &mut dyn Write, layout: &Layout) -> Result<(), Error> {
        let struct_name = rs_pascalcase(&layout.name);
        let width_bytes = layout.width_bytes();
        let trait_prefix = self.trait_prefix();

        let error_type = if self.opts.unpacking_error_msg {
            "&'static str"
        } else {
            "()"
        };

        let mut out = IndentWriter::new(out, "    ");

        // Prevent unused var warnings:
        let val_in_sig = if layout.fields_with_content().count() != 0 {
            "val"
        } else {
            "_val"
        };

        // Impl block and function signature:
        // Depending on if the bytes-to-register conversion can fail, we either
        // generate an 'FromBytes' or 'TryFromBytes' impl.
        if layout.can_always_unpack() {
            writeln!(out)?;
            writeln!(out, "impl {trait_prefix}FromBytes<{width_bytes}> for {struct_name} {{")?;
            writeln!(out, "    fn from_le_bytes({val_in_sig}: [u8; {width_bytes}]) -> Self {{")?;
        } else {
            writeln!(out)?;
            writeln!(out, "impl {trait_prefix}TryFromBytes<{width_bytes}> for {struct_name} {{")?;
            writeln!(out, "    type Error = {error_type};")?;
            writeln!(
                out,
                "    fn try_from_le_bytes({val_in_sig}: [u8; {width_bytes}]) -> Result<Self, Self::Error> {{"
            )?;
        }
        out.increase_indent(2);

        // Sublayouts require a bunch of array wrangling, which is done before the struct initialiser:
        for field in layout.fields_with_content() {
            let FieldType::Layout(sublayout) = &field.accepts else {
                continue;
            };
            writeln!(out, "// {} @ {struct_name}[{}]:", field.name, mask_to_bit_ranges_str(field.mask))?;

            // Assemble field bytes into array:
            let array_len = sublayout.width_bytes();
            let array_name = rs_snakecase(&field.name);

            if sublayout.fields.is_empty() {
                writeln!(out, "let {array_name}: [u8; {array_len}] = [0; {array_len}];")?;
                continue;
            }

            writeln!(out, "let mut {array_name}: [u8; {array_len}] = [0; {array_len}];")?;

            for byte in 0..width_bytes {
                for field_byte in 0..array_len {
                    // Determine required transform to put byte 'byte' of packed input into 'field_byte' of
                    // field:
                    let transform = packed_byte_to_field_byte_transform(
                        Endianess::Little,
                        sublayout.occupied_mask(),
                        lsb_pos(field.mask),
                        field_byte,
                        array_len,
                        byte,
                        width_bytes,
                    );

                    let Some(transform) = transform else {
                        continue;
                    };

                    let masked = if transform.mask != 0xFF {
                        format!("(val[{byte}] & 0x{:X})", transform.mask)
                    } else {
                        format!("val[{byte}]")
                    };
                    let shifted = match &transform.shift {
                        Some((ShiftDirection::Left, amnt)) => format!("{masked} << {amnt}"),
                        Some((ShiftDirection::Right, amnt)) => format!("{masked} >> {amnt}"),
                        None => masked,
                    };

                    writeln!(out, "{array_name}[{field_byte}] |= {};", remove_wrapping_parens(&shifted))?;
                }
            }
        }

        // Struct initialiser to return:
        if layout.can_always_unpack() {
            writeln!(out, "Self {{")?;
        } else {
            writeln!(out, "Ok(Self {{")?;
        }

        for field in layout.fields_with_content() {
            let field_name = rs_snakecase(&field.name);
            writeln!(out, "  // {} @ {struct_name}[{}]:", field.name, mask_to_bit_ranges_str(field.mask))?;

            match &field.accepts {
                FieldType::UInt => {
                    // Numeric fields can be directly converted:
                    let numeric_value = self.assemble_numeric_field(layout, field)?;
                    writeln!(out, "  {field_name}: {numeric_value},")?;
                }
                FieldType::Bool => {
                    // Bools require a simple conversion:
                    let numeric_value = self.assemble_numeric_field(layout, field)?;
                    writeln!(out, "  {field_name}: {numeric_value} != 0,")?;
                }
                FieldType::Enum(e) => {
                    // Enum requires conversion:
                    let numeric_value = self.assemble_numeric_field(layout, field)?;
                    let converted_value = match (field.can_always_unpack(), e.can_unpack_min_bitwidth()) {
                        (true, true) => format!("({numeric_value}).into()"),
                        (true, false) => {
                            if !self.enums_requiring_truncating_conv.contains(&e.name) {
                                panic!("Did not generate truncating conv for enum requiring it");
                            }
                            format!("{}::truncated_from({numeric_value})", rs_pascalcase(&e.name))
                        }
                        (false, _) => format!("({numeric_value}).try_into()?"),
                    };
                    writeln!(out, "  {field_name}: {converted_value},")?;
                }
                FieldType::Layout(layout) => {
                    let layout_name = rs_pascalcase(&layout.name);
                    let array_name = rs_snakecase(&field.name);
                    if field.can_always_unpack() {
                        writeln!(out, "  {field_name}: {layout_name}::from_le_bytes({array_name}),")?;
                    } else {
                        writeln!(out, "  {field_name}: {layout_name}::try_from_le_bytes({array_name})?,")?;
                    };
                }
                FieldType::Fixed(_) => unreachable!(),
            }
        }

        out.decrease_indent(2);
        // Close struct, function and impl:
        if layout.can_always_unpack() {
            writeln!(out, "        }}")?;
        } else {
            writeln!(out, "        }})")?;
        }
        writeln!(out, "    }}")?;
        writeln!(out, "}}")?;
        Ok(())
    }

    fn generate_layout_impl_uint_conv(&self, out: &mut dyn Write, layout: &Layout) -> Result<(), Error> {
        let struct_name = rs_pascalcase(&layout.name);
        let trait_prefix = self.trait_prefix();

        let (uint_type, uint_width_bytes) = match layout.width_bytes() {
            1 => ("u8", 1),
            2 => ("u16", 2),
            3..=4 => ("u32", 4),
            5..=8 => ("u64", 8),
            9..=16 => ("u128", 16),
            _ => return Ok(()),
        };

        let mut out = IndentWriter::new(out, "    ");

        // Struct -> Bytes:

        writeln!(out)?;
        writeln!(out, "impl From<{struct_name}> for {uint_type} {{")?;
        writeln!(out, "    fn from(value: {struct_name}) -> Self {{")?;
        out.increase_indent(2);

        if !trait_prefix.is_empty() {
            writeln!(out, "use {trait_prefix}ToBytes;")?;
        }
        if uint_width_bytes == layout.width_bytes() {
            writeln!(out, "Self::from_le_bytes(value.to_le_bytes())")?;
        } else {
            writeln!(out, "let mut bytes = [0; {uint_width_bytes}];")?;
            writeln!(out, "bytes[0..{}].copy_from_slice(&value.to_le_bytes());", layout.width_bytes())?;
            writeln!(out, "Self::from_le_bytes(bytes)")?;
        }

        out.decrease_indent(2);
        writeln!(out, "    }}")?;
        writeln!(out, "}}")?;

        // Bytes -> Struct:

        if layout.can_always_unpack() {
            writeln!(out)?;
            writeln!(out, "impl From<{uint_type}> for {struct_name} {{")?;
            writeln!(out, "    fn from(value: {uint_type}) -> Self {{")?;
            if !trait_prefix.is_empty() {
                writeln!(out, "        use {trait_prefix}FromBytes;")?;
            }
            if uint_width_bytes == layout.width_bytes() {
                writeln!(out, "        Self::from_le_bytes(value.to_le_bytes())")?;
            } else {
                writeln!(out, "        let mut bytes = [0; {}];", layout.width_bytes())?;
                writeln!(out, "        bytes.copy_from_slice(&(value.to_le_bytes()[0..{}]));", layout.width_bytes())?;
                writeln!(out, "        Self::from_le_bytes(bytes)")?;
            }
            writeln!(out, "    }}")?;
            writeln!(out, "}}")?;
        } else {
            writeln!(out)?;
            writeln!(out, "impl TryFrom<{uint_type}> for {struct_name} {{")?;
            if self.opts.unpacking_error_msg {
                writeln!(out, "    type Error = &'static str;")?;
            } else {
                writeln!(out, "    type Error = ();")?;
            }
            writeln!(out, "    fn try_from(value: {uint_type}) -> Result<Self, Self::Error> {{")?;
            if !trait_prefix.is_empty() {
                writeln!(out, "        use {trait_prefix}TryFromBytes;")?;
            }
            if uint_width_bytes == layout.width_bytes() {
                writeln!(out, "        Self::try_from_le_bytes(value.to_le_bytes())")?;
            } else {
                writeln!(out, "        let mut bytes = [0; {}];", layout.width_bytes())?;
                writeln!(out, "        bytes.copy_from_slice(&(value.to_le_bytes()[0..{}]));", layout.width_bytes())?;
                writeln!(out, "        Self::try_from_le_bytes(bytes)")?;
            }
            writeln!(out, "    }}")?;
            writeln!(out, "}}")?;
        }

        Ok(())
    }

    /// Generate register section header comment
    fn generate_register_header(out: &mut dyn Write, register: &Register) -> Result<(), Error> {
        let name = &register.name;
        writeln!(out)?;
        rs_generate_section_header_comment(out, &format!("{name} Register"))?;
        if !register.docs.is_empty() {
            write!(out, "{}", register.docs.as_multiline("// "))?;
            writeln!(out, "//")?;
        }
        writeln!(out, "// Fields:")?;
        writeln!(out, "{}", rs_layout_overview_comment(&register.layout))?;
        Ok(())
    }

    fn generate_register_properties(&self, out: &mut dyn Write, register: &Register) -> Result<(), Error> {
        let reg_name = &register.name;
        let const_reg_name = rs_const(reg_name);
        let byte_width = register.layout.width_bytes();
        let byte_array = format!("[u8; {byte_width}]");
        let address_type = &self.address_type;

        // Address
        writeln!(out)?;
        writeln!(out, "/// `{reg_name}` register address")?;
        writeln!(out, "pub const {const_reg_name}_ADDRESS: {address_type} = 0x{:x};", register.adr)?;

        // Reset val.
        if let Some(reset_val) = &register.reset_val {
            let val = Self::to_array_literal(Endianess::Little, *reset_val, byte_width);
            writeln!(out)?;
            writeln!(out, "/// `{reg_name}` little-endian reset value")?;
            writeln!(out, "pub const {const_reg_name}_RESET_LE: {byte_array} = {val};")?;

            let val = Self::to_array_literal(Endianess::Big, *reset_val, byte_width);
            writeln!(out)?;
            writeln!(out, "/// `{reg_name}` big-endian reset value")?;
            writeln!(out, "pub const {const_reg_name}_RESET_BE: {byte_array} = {val};")?;
        }

        Ok(())
    }

    fn generate_register_block_header(out: &mut dyn Write, block: &RegisterBlock) -> Result<(), Error> {
        let name = &block.name;
        writeln!(out)?;
        rs_generate_section_header_comment(out, &format!("{name} Register Block"))?;
        if !block.docs.is_empty() {
            write!(out, "{}", block.docs.as_multiline("// "))?;
        }

        if !block.members.is_empty() {
            writeln!(out, "//")?;
            writeln!(out, "// Contains registers:")?;
            for member in block.members.values() {
                if let Some(brief) = &member.docs.brief {
                    writeln!(out, "// - `[0x{:02}]` {}: {}", member.offset, member.name, brief)?;
                } else {
                    writeln!(out, "// - `[0x{:02}]` {}", member.offset, member.name)?;
                }
            }
        }

        if !block.instances.is_empty() {
            writeln!(out, "//")?;
            writeln!(out, "// Instances:")?;
            for instance in block.instances.values() {
                if let Some(brief) = &instance.docs.brief {
                    writeln!(out, "// - `[0x{:02}]` {}: {}", instance.adr, instance.name, brief)?;
                } else {
                    writeln!(out, "// - `[0x{:02}]` {}", instance.adr, instance.name)?;
                }
            }
        }

        Ok(())
    }

    fn generate_register_block_properties(&self, out: &mut dyn Write, block: &RegisterBlock) -> Result<(), Error> {
        let address_type = &self.address_type;
        let block_name = &block.name;
        let const_block_name = rs_const(block_name);

        if !block.members.is_empty() {
            writeln!(out)?;
            writeln!(out, "// Contained registers:")?;
            for member in block.members.values() {
                let reg_name = &member.name;
                let const_reg_name = rs_const(reg_name);

                writeln!(out)?;
                writeln!(out, "/// Offset of `{reg_name}` register from `{block_name}` block start")?;
                writeln!(out, "pub const {const_reg_name}_OFFSET: {address_type} = 0x{:x};", member.offset)?;
            }
        }

        if !block.instances.is_empty() {
            writeln!(out)?;
            writeln!(out, "// Instances:")?;
            for instance in block.instances.values() {
                let instance_name = &instance.name;
                let const_instance_name = rs_const(instance_name);
                writeln!(out)?;
                writeln!(out, "/// Start of `{block_name}` instance `{instance_name}`")?;
                writeln!(
                    out,
                    "pub const {const_block_name}_INSTANCE_{const_instance_name}: {address_type} = 0x{:x};",
                    instance.adr
                )?;
            }
        }

        Ok(())
    }

    fn generate_register_block_member_header(out: &mut dyn Write, member: &RegisterBlockMember) -> Result<(), Error> {
        let name = &member.name;
        writeln!(out)?;
        rs_generate_header_comment(out, &format!("{name} Register Block Member "))?;

        if !member.docs.is_empty() {
            write!(out, "{}", member.docs.as_multiline("// "))?;
            writeln!(out, "//")?;
        }

        writeln!(out, "// Fields:")?;
        writeln!(out, "{}", rs_layout_overview_comment(&member.layout))?;
        Ok(())
    }

    fn generate_register_block_member_properties(
        &self,
        out: &mut dyn Write,
        block: &RegisterBlock,
        member: &RegisterBlockMember,
    ) -> Result<(), Error> {
        let address_type = &self.address_type;
        let byte_width = member.layout.width_bytes();
        let byte_array = format!("[u8; {byte_width}]");

        for block_instance in block.instances.values() {
            let member_instance = &block_instance.registers[&member.name];

            let reg_name = &member_instance.name;
            let const_reg_name = rs_const(reg_name);

            // Address
            writeln!(out)?;
            writeln!(out, "/// `{reg_name}` register address")?;
            writeln!(out, "pub const {const_reg_name}_ADDRESS: {address_type} = 0x{:x};", member_instance.adr)?;

            // Reset value:
            if let Some(reset_val) = &member_instance.reset_val {
                let val = Self::to_array_literal(Endianess::Little, *reset_val, byte_width);
                writeln!(out)?;
                writeln!(out, "/// `{reg_name}` little-endian reset value")?;
                writeln!(out, "pub const {const_reg_name}_RESET_LE: {byte_array} = {val};")?;

                let val = Self::to_array_literal(Endianess::Big, *reset_val, byte_width);
                writeln!(out)?;
                writeln!(out, "/// `{reg_name}` big-endian reset value")?;
                writeln!(out, "pub const {const_reg_name}_RESET_BE: {byte_array} = {val};")?;
            }
        }
        Ok(())
    }

    fn assemble_numeric_field(&self, layout: &Layout, field: &LayoutField) -> Result<String, Error> {
        let field_raw_type = match &field.accepts {
            FieldType::UInt => self.register_layout_member_type(field)?,
            FieldType::Bool => "u8".to_string(),
            FieldType::Enum(e) => rs_fitting_unsigned_type(e.min_bitdwith())?,
            FieldType::Fixed(_) => unreachable!(),
            FieldType::Layout(_) => unreachable!(),
        };

        let mut unpacked_value_parts: Vec<String> = vec![];

        for byte in 0..layout.width_bytes() {
            let Some(transform) = packed_byte_to_field_transform(
                Endianess::Little,
                unpositioned_mask(field.mask),
                lsb_pos(field.mask),
                byte,
                layout.width_bytes(),
            ) else {
                continue;
            };

            let casted_value = if field_raw_type == "u8" {
                format!("val[{byte}]")
            } else {
                format!("{field_raw_type}::from(val[{byte}])")
            };

            let masked = if transform.mask == 0xFF {
                casted_value
            } else {
                format!("({casted_value} & 0x{:X})", transform.mask)
            };

            match &transform.shift {
                Some((ShiftDirection::Left, amnt)) => unpacked_value_parts.push(format!("{masked} << {amnt}")),
                Some((ShiftDirection::Right, amnt)) => unpacked_value_parts.push(format!("{masked} >> {amnt}")),
                None => unpacked_value_parts.push(masked),
            };
        }
        assert!(!unpacked_value_parts.is_empty());

        Ok(remove_wrapping_parens(&unpacked_value_parts.join(" | ")))
    }

    fn trait_prefix(&self) -> String {
        // Decide trait prefix. If an external override is given, use that.
        // Otherwise, use the local definition.
        self.opts.external_traits.as_ref().cloned().unwrap_or(String::new())
    }

    /// Convert a value to an array literal of given endianess
    fn to_array_literal(endian: Endianess, val: TypeValue, width_bytes: TypeBitwidth) -> String {
        let mut bytes: Vec<String> = vec![];

        for i in 0..width_bytes {
            let byte = format!("0x{:X}", ((val >> (8 * i)) & 0xFF) as u8);
            bytes.push(byte);
        }

        if matches!(endian, Endianess::Big) {
            bytes.reverse();
        }

        format!("[{}]", bytes.join(", "))
    }
}