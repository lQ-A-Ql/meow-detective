//! Offline whitelist extractor: Microsoft PDB -> minimal JSON symbol table.
//!
//! Usage: pdb-symbol-extract <module.json> <input.pdb> <output.json>
//! The module config selects which globals (public symbols) and which struct
//! layouts are extracted. Everything else in the PDB is ignored.

use pdb::{FallibleIterator, ItemFinder, SymbolData, TypeData, PDB};
use serde_json::{json, Map, Value};
use std::collections::BTreeMap;
use std::fs::File;

fn main() -> pdb::Result<()> {
    let mut args = std::env::args().skip(1);
    let config_path = args.next().expect("usage: extract <config> <pdb> <out>");
    let pdb_path = args.next().expect("pdb path");
    let out_path = args.next().expect("out path");

    let config: Value =
        serde_json::from_str(&std::fs::read_to_string(&config_path).expect("read config"))
            .expect("parse config");
    let wanted_globals: Vec<String> = config["globals"]
        .as_array()
        .unwrap_or(&vec![])
        .iter()
        .filter_map(|v| v.as_str().map(str::to_string))
        .collect();
    let global_prefixes: Vec<String> = config["globalPrefixes"]
        .as_array()
        .unwrap_or(&vec![])
        .iter()
        .filter_map(|v| v.as_str().map(str::to_string))
        .collect();
    let wanted_types: Vec<String> = config["types"]
        .as_array()
        .unwrap_or(&vec![])
        .iter()
        .filter_map(|v| v.as_str().map(str::to_string))
        .collect();
    let type_prefixes: Vec<String> = config["typePrefixes"]
        .as_array()
        .unwrap_or(&vec![])
        .iter()
        .filter_map(|v| v.as_str().map(str::to_string))
        .collect();

    let file = File::open(&pdb_path).expect("open pdb");
    let mut pdb = PDB::open(file)?;

    let info = pdb.pdb_information()?;
    let pdb_guid = format!("{:X}", info.guid);
    let pdb_age = info.age;

    // --- globals: public symbol RVA = section virtual address + offset ---
    let sections = pdb.sections().ok().flatten().unwrap_or_default();
    let mut globals = BTreeMap::new();
    let symbols = pdb.global_symbols()?;
    let mut iter = symbols.iter();
    while let Some(symbol) = iter.next()? {
        if let Ok(SymbolData::Public(data)) = symbol.parse() {
            let name = data.name.to_string().into_owned();
            let wanted = wanted_globals.iter().any(|wanted| wanted == &name)
                || global_prefixes
                    .iter()
                    .any(|prefix| name.starts_with(prefix.as_str()));
            if !wanted {
                continue;
            }
            let section = usize::from(data.offset.section);
            let base = sections
                .get(section.saturating_sub(1))
                .map(|header| u64::from(header.virtual_address))
                .unwrap_or(0);
            let rva = base + u64::from(data.offset.offset);
            globals.insert(
                name,
                json!({ "rva": rva, "rvaHex": format!("0x{rva:X}"), "code": data.code }),
            );
        }
    }

    // --- types: struct/union layouts with field offsets ---
    let type_information = pdb.type_information()?;
    let mut finder = type_information.finder();
    let mut type_iter = type_information.iter();
    let mut layouts = Map::new();
    let list_only = config["listTypes"].as_bool().unwrap_or(false);
    let mut all_names = BTreeMap::new();
    while let Some(typ) = type_iter.next()? {
        finder.update(&type_iter);
        let parsed = match typ.parse() {
            Ok(parsed) => parsed,
            Err(_) => continue,
        };
        let (name, fields_index, size, kind) = match &parsed {
            TypeData::Class(class) => (
                class.name.to_string().into_owned(),
                class.fields,
                class.size,
                "struct",
            ),
            TypeData::Union(union) => (
                union.name.to_string().into_owned(),
                Some(union.fields),
                union.size,
                "union",
            ),
            _ => continue,
        };
        let wanted = wanted_types.iter().any(|wanted| wanted == &name)
            || type_prefixes.iter().any(|prefix| name.starts_with(prefix.as_str()));
        if list_only {
            if !name.starts_with('<') {
                all_names.insert(name, u64::from(size));
            }
            continue;
        }
        if !wanted {
            continue;
        }
        let mut fields = Vec::new();
        if let Some(field_list) = fields_index {
            if let Ok(TypeData::FieldList(field_list)) = finder.find(field_list)?.parse() {
                for field in field_list.fields {
                    if let TypeData::Member(member) = field {
                        fields.push(json!({
                            "name": member.name.to_string(),
                            "offset": member.offset,
                            "offsetHex": format!("0x{:X}", member.offset),
                        }));
                    }
                }
            }
        }
        // Forward declarations (size 0, no fields) appear before the real
        // definition in the type stream; never let them shadow it.
        let dominated = layouts
            .get(&name)
            .is_some_and(|existing| existing["size"].as_u64().unwrap_or(0) >= u64::from(size));
        if dominated {
            continue;
        }
        layouts.insert(
            name,
            json!({
                "kind": kind,
                "size": size,
                "fields": fields,
            }),
        );
    }

    let output = json!({
        "module": config["module"],
        "pdbGuid": pdb_guid,
        "pdbAge": pdb_age,
        "source": "Microsoft public symbol server (msdl.microsoft.com/download/symbols)",
        "globals": globals,
        "types": Value::Object(layouts),
    });
    if list_only {
        let names: Vec<String> = all_names
            .iter()
            .map(|(name, size)| format!("{name} (0x{size:X})"))
            .collect();
        std::fs::write(&out_path, names.join("\n")).expect("write names");
        eprintln!("listed {} type names -> {out_path}", names.len());
        return Ok(());
    }
    std::fs::write(&out_path, serde_json::to_string_pretty(&output).expect("serialize"))
        .expect("write output");
    eprintln!(
        "{}: {} globals, {} types -> {out_path}",
        config["module"].as_str().unwrap_or("?"),
        output["globals"].as_object().map_or(0, |m| m.len()),
        output["types"].as_object().map_or(0, |m| m.len()),
    );
    Ok(())
}
