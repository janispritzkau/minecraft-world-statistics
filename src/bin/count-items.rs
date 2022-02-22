use std::io::{self, BufRead, BufReader};

use indexmap::IndexMap;
use quartz_nbt::{NbtCompound, NbtList};

fn main() -> eyre::Result<()> {
    let mut total_items = IndexMap::new();

    for line in BufReader::new(io::stdin()).lines() {
        let line = line?;

        let item = quartz_nbt::snbt::parse(&line)?;
        let id = item.get::<_, &String>("id")?;
        let count = item.get::<_, u8>("Count")?;
        *total_items.entry(id.clone()).or_insert(0) += count as u64;

        if id.ends_with("shulker_box") && item.contains_key("tag") {
            let tag: &NbtCompound = item.get("tag")?;
            if tag.contains_key("BlockEntityTag") {
                let block_entity_tag: &NbtCompound = tag.get("BlockEntityTag")?;
                if block_entity_tag.contains_key("Items") {
                    let items: &NbtList = block_entity_tag.get("Items")?;
                    for item in items.iter_map::<&NbtCompound>() {
                        let item = item?;
                        let id = item.get::<_, &String>("id")?;
                        let count = item.get::<_, u8>("Count")?;
                        *total_items.entry(id.clone()).or_insert(0) += count as u64;
                    }
                }
            }
        }
    }

    total_items.sort_by(|_, a, _, b| b.cmp(a));
    println!("{}", serde_json::to_string_pretty(&total_items)?);

    Ok(())
}
