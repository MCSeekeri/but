use std::{
    collections::HashMap,
    env, fs,
    fs::File,
    io::{BufWriter, Read, Seek, Write},
    path::{Path, PathBuf},
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use anyhow::Context;
use serde::{Deserialize, Serialize};
use clap::Parser;
use zip::{write::SimpleFileOptions, ZipWriter};
use walkdir::WalkDir;
use tar::Builder;
use zstd::stream::Encoder;

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "lowercase")]
enum Compression {
    Zip,
    Zstd,
}

#[derive(Serialize, Deserialize, Debug)]
struct Backup {
    from: String,
    dest: String,
}

#[derive(Serialize, Deserialize, Debug)]
struct Config {
    settings: Settings,
    backup: HashMap<String, Backup>,
}

#[derive(Serialize, Deserialize, Debug)]
struct Settings {
    interval: u64,
    filename: String,
    compression: Compression,
}

fn init_config() -> Result<(), Box<dyn std::error::Error>> {
    let default_config = Config {
        settings: Settings {
            interval: 300,
            filename: "%name%-%timestamp%".to_string(),
            compression: Compression::Zip,
        },
        backup: HashMap::from([
            (
                "path1".to_string(),
                Backup {
                    from: "/a/path".to_string(),
                    dest: "./".to_string(),
                },
            ),
            (
                "path2".to_string(),
                Backup {
                    from: "/another/path".to_string(),
                    dest: "./".to_string(),
                },
            ),
        ]),
    };
    write_config_file(&default_config)?;
    Ok(())
}

fn write_config_file(config: &Config) -> Result<(), Box<dyn std::error::Error>> {
    let toml = toml::to_string(config)?;
    fs::write("but.conf", toml)?;
    Ok(())
}

fn load_config() -> Result<Config, Box<dyn std::error::Error>> {
    let config_paths = vec![
        PathBuf::from("/etc/but.conf"),
        PathBuf::from(format!("{}/.config/but.conf", env::var("HOME").unwrap_or(".".to_string()))),
        PathBuf::from("but.conf"),
    ];

    for config_path in config_paths {
        if config_path.exists() {
            let config_str = fs::read_to_string(config_path)?;
            let config: Config = toml::from_str(&config_str)?;
            return Ok(config);
        }
    }

    Err("找不到配置文件".into())
}

fn start_listen(verbose: bool) {
    let config = load_config().unwrap_or_else(|err| {
        eprintln!("加载配置失败: {}", err);
        std::process::exit(1);
    });

    let mut last_backup_time = SystemTime::now();
    let mut last_meta = HashMap::new();
    let mut no_change_notified = HashMap::new();

    loop {
        std::thread::sleep(Duration::from_secs(1));
        let now = SystemTime::now();
        if now.duration_since(last_backup_time).unwrap().as_secs() >= config.settings.interval {
            for (name, item) in config.backup.iter() {
                let mut changed_files = Vec::new();
                let mut current_meta = HashMap::new();

                if !Path::new(&item.from).exists() {
                    eprintln!("目录 {} 不存在，跳过当前备份。", item.from);
                    continue;
                }

                for entry in WalkDir::new(&item.from) {
                    let entry: walkdir::Result<walkdir::DirEntry> = entry;
                    let entry = entry.unwrap();
                    let path = entry.path();
                    if path.is_file() {
                        let meta = fs::metadata(path).unwrap();
                        let last_modified = meta.modified().unwrap().duration_since(UNIX_EPOCH).unwrap().as_secs();
                        current_meta.insert(path.to_string_lossy().to_string(), last_modified);
                
                        if let Some(last_modified_old) = last_meta.get(name).and_then(|meta: &HashMap<String, u64>| meta.get(&path.to_string_lossy().to_string())) {    if *last_modified_old != last_modified {
                                changed_files.push(path.to_string_lossy().to_string());
                            }
                        } else {
                            changed_files.push(path.to_string_lossy().to_string());
                        }
                    }
                }

                if changed_files.is_empty() {
                    if !no_change_notified.get(name).unwrap_or(&false) {
                        println!("{} 没有检测到文件更改，会在文件修改时继续备份", name);
                        no_change_notified.insert(name.clone(), true);
                    }
                } else {
                    println!("正在备份 {}", name);
                    if verbose {
                        println!("自上次备份至今变动的文件列表:");
                        for file in changed_files {
                            println!("{}", file);
                        }
                    }

                    let backup_name = format!(
                        "{}-{}",
                        name.replace("%name%", name),
                        SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs()
                    );

                    let dest_path = format!(
                        "{}/{}.{}",
                        item.dest,
                        backup_name,
                        match config.settings.compression {
                            Compression::Zip => "zip",
                            Compression::Zstd => "tar.zst",
                        }
                    );

                    match compress(item.from.as_str(), dest_path.as_str(), &config.settings.compression) {
                        Ok(_) => {
                            println!("备份成功。");
                        }
                        Err(e) => {
                            eprintln!("备份失败: {}", e);
                        }
                    }

                    last_meta.insert(name.clone(), current_meta);
                    no_change_notified.insert(name.clone(), false);
                }
            }

            last_backup_time = now;
        }
    }
}

fn compress(from: &str, target_file: &str, compression: &Compression) -> Result<(), Box<dyn std::error::Error>> {
    match compression {
        Compression::Zip => compress_dir(from, target_file),
        Compression::Zstd => compress_dir_zstd(from, target_file),
    }
}

fn compress_dir_zstd(from: &str, target_file: &str) -> Result<(), Box<dyn std::error::Error>> {
    let file = File::create(target_file)?;
    let encoder = Encoder::new(file, 3)?;
    let mut builder = Builder::new(encoder);

    for entry in WalkDir::new(from) {
        let entry = entry?;
        let path = entry.path();
        if path.is_file() {
            builder.append_path_with_name(path, path.strip_prefix(from).unwrap())?;
        }
    }

    builder.into_inner()?.finish()?;
    Ok(())
}

fn compress_dir(from: &str, target_file: &str) -> Result<(), Box<dyn std::error::Error>> {
    let file = File::create(target_file)?;
    let mut zip = ZipWriter::new(BufWriter::new(file));
    let options = SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .unix_permissions(0o755);

    zip_dir(WalkDir::new(from), Path::new(from), &mut zip, options)?;
    zip.finish()?;
    Ok(())
}

fn zip_dir<T>(
    it: WalkDir,
    prefix: &Path,
    writer: &mut ZipWriter<T>,
    options: SimpleFileOptions,
) -> anyhow::Result<()>
where
    T: Write + Seek,
{
    let prefix = Path::new(prefix);
    let mut buffer = Vec::new();
    for entry in it {
        let entry = entry?;
        let path = entry.path();
        let name = path.strip_prefix(prefix).unwrap();
        let path_as_string = name
            .to_str()
            .map(str::to_owned)
            .with_context(|| format!("{name:?} 是非法的 UTF-8 路径"))?;

        if path.is_file() {
            writer.start_file(path_as_string, options)?;
            let mut f = File::open(path)?;

            f.read_to_end(&mut buffer)?;
            writer.write_all(&buffer)?;
            buffer.clear();
        } else if !name.as_os_str().is_empty() {
            writer.add_directory(path_as_string, options)?;
        }
    }
    Ok(())
}

#[derive(Parser, Debug)]
#[clap(
    name = "BinUntilTrash",
    version = "0.0.1",
    author = "MCSeekeri",
    about = "But it's just another file backup tool.",
    disable_help_flag = true
)]
struct Args {
    #[clap(short = 'v', long, help = "启用详细输出")]
    verbose: bool,

    #[clap(short = 'i', long, help = "初始化配置文件")]
    init: bool,
}

fn main() {
    let args = Args::parse();

    if args.init {
        if let Err(err) = init_config() {
            eprintln!("初始化配置失败: {}", err);
        }
    } else {
        start_listen(args.verbose);
    }
}
