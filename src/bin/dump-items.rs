use std::{
    collections::{HashMap, HashSet},
    fs::{self, DirEntry, File},
    io,
    path::{Path, PathBuf},
    sync::Arc,
};

use clap::Parser;
use eyre::Context;
use quartz_nbt::{NbtCompound, NbtList};
use regex::Regex;
use world_statistics::region::{read_chunk, RegionFile};

/// Dumps the items in a world line seperated in SNBT
#[derive(Parser, Debug)]
#[clap(color = clap::ColorChoice::Never)]
struct Args {
    /// item_frame, hopper_minecart, etc.
    #[clap(short, long, default_value = "all")]
    entities: String,

    /// barrel, chest, hopper, etc.
    #[clap(short, long, default_value = "all")]
    block_entities: String,

    /// Path to the world directory
    world: String,

    /// overworld, nether, end, playerdata
    #[clap(required = true)]
    sources: Vec<String>,
}

const ENTITY_IDS: &[&str] = &[
    "minecraft:item",
    "minecraft:item_frame",
    "minecraft:glow_item_frame",
    "minecraft:chest_minecart",
    "minecraft:hopper_minecart",
];

const BLOCK_ENTITY_IDS: &[&str] = &[
    "minecraft:barrel",
    "minecraft:blast_furnace",
    "minecraft:dispenser",
    "minecraft:dropper",
    "minecraft:chest",
    "minecraft:furnace",
    "minecraft:hopper",
    "minecraft:shulker_box",
    "minecraft:smoker",
    "minecraft:trapped_chest",
];

fn main() -> eyre::Result<()> {
    let args = Args::parse();

    let world_path = PathBuf::from(args.world);

    for source in args.sources {
        let mut split = source.split(":");

        let name = split.next().unwrap();
        let opts = parse_opts(split.next());

        match name {
            "overworld" | "nether" | "end" => {
                let dim_path = world_path.join(match name {
                    "overworld" => "",
                    "nether" => "DIM-1",
                    "end" => "DIM1",
                    _ => panic!(),
                });

                scan_dimension(ScanDimensionOptions {
                    dim_path,
                    entities: parse_list(&args.entities, ENTITY_IDS),
                    block_entities: parse_list(&args.block_entities, BLOCK_ENTITY_IDS),
                    chunk_radius: opts
                        .get("chunk_radius")
                        .map(|&str| str.parse().ok())
                        .flatten(),
                })?;
            }
            "playerdata" => {
                scan_playerdata(ScanPlayerDataOptions {
                    inventory: if opts.is_empty() {
                        true
                    } else {
                        opts.contains_key("inventory")
                    },
                    ender_chest: if opts.is_empty() {
                        true
                    } else {
                        opts.contains_key("ender_chest")
                    },
                });
            }
            name => panic!("unknown source: {}", name),
        }
    }

    Ok(())
}

#[derive(Debug)]
pub struct ScanDimensionOptions {
    pub dim_path: PathBuf,
    pub entities: HashSet<String>,
    pub block_entities: HashSet<String>,
    pub chunk_radius: Option<u32>,
}

fn scan_dimension(options: ScanDimensionOptions) -> eyre::Result<()> {
    let options = Arc::new(options);
    let region_regex = Regex::new(r"^r\.(-?\d+)\.(-?\d+)\.mca$")?;
    let region_path = options.dim_path.join("region");

    let mut region_files: Vec<(i32, i32, DirEntry)> = fs::read_dir(region_path)
        .context("region file folder not found")?
        .flatten()
        .map(|entry| {
            let filename = entry.file_name();
            let cap = region_regex.captures(filename.to_str()?)?;
            Some((cap[1].parse().ok()?, cap[2].parse().ok()?, entry))
        })
        .flatten()
        .collect();

    region_files.sort_by_key(|(x, z, _)| (i32::max((x * 2 + 1).abs(), (z * 2 + 1).abs()), *x, *z));

    let (chunk_tx, chunk_rx) = crossbeam_channel::bounded::<(bool, Vec<u8>)>(6);
    let (item_tx, item_rx) = std::sync::mpsc::channel();

    for _ in 0..4 {
        let chunk_rx = chunk_rx.clone();
        let item_tx = item_tx.clone();
        let options = options.clone();

        std::thread::spawn(move || {
            for (is_entity_chunk, buf) in chunk_rx {
                let chunk = read_chunk(&buf).unwrap();

                if is_entity_chunk {
                    let entities = chunk
                        .get::<_, &NbtList>("Entities")
                        .unwrap()
                        .iter_map::<&NbtCompound>();

                    for entity in entities {
                        let entity = entity.unwrap();

                        let id: &str = entity.get("id").unwrap();
                        if !options.entities.contains(id) {
                            continue;
                        }

                        match id {
                            "minecraft:item"
                            | "minecraft:item_frame"
                            | "minecraft:glow_item_frame" => {
                                if entity.contains_key("Item") {
                                    let item: &NbtCompound = entity.get("Item").unwrap();
                                    item_tx.send(item.clone()).unwrap();
                                }
                            }
                            "minecraft:chest_minecart" | "minecraft:hopper_minecart" => {
                                if entity.contains_key("Items") {
                                    let items: &NbtList = entity.get("Items").unwrap();
                                    for item in items.iter_map::<&NbtCompound>() {
                                        item_tx.send(item.unwrap().clone()).unwrap();
                                    }
                                }
                            }
                            _ => panic!(),
                        }
                    }
                } else {
                    let block_entities: &NbtList = chunk.get("block_entities").unwrap();
                    for block_entity in block_entities.iter_map::<&NbtCompound>() {
                        let block_entity = block_entity.unwrap();

                        let id: &str = block_entity.get("id").unwrap();
                        if options.block_entities.contains(id) && block_entity.contains_key("Items")
                        {
                            let items: &NbtList = block_entity.get("Items").unwrap();
                            for item in items.iter_map::<&NbtCompound>() {
                                item_tx.send(item.unwrap().clone()).unwrap();
                            }
                        }
                    }
                }
            }
        });
    }

    let handle = std::thread::spawn(move || {
        for item in item_rx {
            println!("{}", item);
        }
    });

    for (region_x, region_z, entry) in region_files.into_iter() {
        if let Some(chunk_radius) = options.chunk_radius {
            let r = (chunk_radius as i32 - 1) / 32;
            if region_x > r || region_x < -r - 1 || region_z > r || region_z < -r - 1 {
                continue;
            }
        }

        eprintln!("processing region {} {}", region_x, region_z);

        let scan_region_file = |is_entity_chunk: bool, path: &Path| {
            let mut region_file =
                match RegionFile::new(File::open(path).context("region file not found")?) {
                    Ok(region_file) => region_file,
                    Err(e) => match e.kind() {
                        io::ErrorKind::UnexpectedEof => {
                            eprintln!("unexpected eof while reading region file");
                            return Ok(());
                        }
                        _ => eyre::bail!(e),
                    },
                };

            region_file.for_each_chunk(|(index, buf)| {
                let chunk_x = region_x * 32 + (index % 32) as i32;
                let chunk_z = region_z * 32 + (index / 32) as i32;

                if let Some(chunk_radius) = options.chunk_radius {
                    if i32::max((chunk_x * 2 + 1).abs(), (chunk_z * 2 + 1).abs()) as u32
                        > 2 * chunk_radius
                    {
                        return;
                    }
                }

                chunk_tx.send((is_entity_chunk, buf.to_vec())).unwrap();
            })?;

            Ok(())
        };

        let entity_region_path = options.dim_path.join("entities").join(entry.file_name());
        scan_region_file(false, &entry.path())?;
        scan_region_file(true, &entity_region_path)?;
    }

    drop(chunk_tx);
    drop(item_tx);
    handle.join().unwrap();

    Ok(())
}

#[derive(Debug)]
pub struct ScanPlayerDataOptions {
    pub inventory: bool,
    pub ender_chest: bool,
}

fn scan_playerdata(_options: ScanPlayerDataOptions) {
    unimplemented!()
}

fn parse_list(list: &str, default: &[&str]) -> HashSet<String> {
    if list == "all" {
        HashSet::from_iter(default.iter().map(|str| str.to_string()))
    } else {
        HashSet::from_iter(list.split(",").map(|str| String::from("minecraft:") + str))
    }
}

fn parse_opts(opts: Option<&str>) -> HashMap<&str, &str> {
    let mut map = HashMap::new();
    if let Some(opts) = opts {
        for opt in opts.split(",") {
            if !opt.contains("=") {
                map.insert(opt, "");
                continue;
            }
            let [key, value]: [&str; 2] =
                opt.splitn(2, "=").collect::<Vec<_>>().try_into().unwrap();
            map.insert(key, value);
        }
    }
    map
}
