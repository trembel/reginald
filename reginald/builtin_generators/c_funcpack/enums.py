from typing import List

from reginald.datamodel import *
from reginald.generator import OutputGenerator
from reginald.utils import c_sanitize


class Generator(OutputGenerator):
    @classmethod
    def description(cls):
        return "TODO"

    @classmethod
    def generate(cls, map: RegisterMap, args: List[str]):
        devname = map.device_name

        devname_macro = c_sanitize(devname).upper()
        devname_c = c_sanitize(devname).lower()

        out = []

        out.append(f"/*")
        out.append(f"* {devname} Register Enums.")
        out.append(f"* Note: do not edit: Generated using Reginald.")
        out.append(f"*/")
        out.append(f"")
        out.append(f"#ifndef {devname_macro}_REG_ENUMS_H_")
        out.append(f"#define {devname_macro}_REG_ENUMS_H_")
        out.append(f"")

        title_line = f"// ==== Global Enums "
        out.append(f"")

        if len(title_line) < 80:
            title_line += ("=" * (80 - len(title_line)))
        out.append(title_line)
        out.append(f"")

        for enumname_orig, enum in map.enums.items():
            enumname_c = c_sanitize(enumname_orig).lower()
            enumname_macro = c_sanitize(enumname_orig).upper()
            out.append(f"typedef enum {{")

            for entryname_orig, entry in enum.items():
                entryname_macro = c_sanitize(entryname_orig).upper()
                out.append(f"  {devname_macro}_{enumname_macro}_{entryname_macro} = 0x{entry.value:X}U,")

            out.append(f"}} {devname_c}_{enumname_c}_t;")
            out.append(f"")

        for registername_orig, reg in map.registers.items():

            enum_count = len([field.enum for field in reg.fields.values() if field.enum is not None])

            if enum_count == 0:
                continue

            title_line = f"// ==== {registername_orig} Enums "
            out.append(f"")

            if len(title_line) < 80:
                title_line += ("=" * (80 - len(title_line)))
            out.append(title_line)
            out.append(f"")

            for fieldname_orig, field in reg.fields.items():
                fieldname_c = c_sanitize(fieldname_orig).lower()
                fieldname_macro = c_sanitize(fieldname_orig).upper()

                if field.enum is not None:
                    out.append(f"typedef enum {{")

                    for entryname_orig, entry in field.enum.items():
                        entryname_macro = c_sanitize(entryname_orig).upper()
                        out.append(f"  {devname_macro}_{fieldname_macro}_{entryname_macro} = 0x{entry.value:X}U,")

                    out.append(f"}} {devname_c}_{fieldname_c}_t;")
                    out.append(f"")

        out.append(f"#endif /* {devname_macro}_REG_ENUMS_H_ */")
        return "\n".join(out)
