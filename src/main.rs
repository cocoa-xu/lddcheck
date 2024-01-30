use clap::Parser;
use elf::endian::AnyEndian;
use elf::parse::ParsingTable;
use elf::string_table::StringTable;
use elf::symbol::Symbol;
use elf::ElfBytes;
use lddtree::{DependencyAnalyzer, Library};
use serde_json;
use std::collections::{HashMap, HashSet};
use std::error::Error;
use std::path::PathBuf;
use std::vec::Vec;
use strum::{Display, EnumCount, EnumDiscriminants, EnumString, VariantNames};
use strum_macros::EnumIs;

#[macro_export]
macro_rules! clap_enum_variants {
    ($e: ty) => {{
        use clap::builder::TypedValueParser;
        clap::builder::PossibleValuesParser::new(<$e>::VARIANTS).map(|s| s.parse::<$e>().unwrap())
    }};
}

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Hash,
    EnumString,
    EnumCount,
    EnumDiscriminants,
    EnumIs,
    Display,
    VariantNames,
    Default,
)]
enum DetailLevel {
    #[strum(serialize = "version")]
    #[default]
    Version,
    #[strum(serialize = "function")]
    Function,
    #[strum(serialize = "file")]
    File,
}

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Hash,
    EnumString,
    EnumCount,
    EnumDiscriminants,
    EnumIs,
    Display,
    VariantNames,
    Default,
)]
enum StdoutFormat {
    #[strum(serialize = "json")]
    Json,
    #[strum(serialize = "text")]
    #[default]
    Text,
}

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Hash,
    EnumString,
    EnumCount,
    EnumDiscriminants,
    EnumIs,
    Display,
    VariantNames,
    Default,
)]
enum PrintError {
    #[strum(serialize = "cannot-parse")]
    CannotParse,
    #[strum(serialize = "cannot-read")]
    CannotRead,
    #[strum(serialize = "not-found")]
    NotFound,
    #[strum(serialize = "none")]
    None,
    #[strum(serialize = "all")]
    #[default]
    All,
}

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[arg(
        short,
        long,
        required = true,
        help = "The path(s) to the file(s) for analysis"
    )]
    paths: Vec<String>,

    #[arg(
        long,
        default_value = "/",
        help = "The root path to use when resolving paths"
    )]
    root: String,

    #[arg(
        short,
        long,
        help = "Additional LD_LIBRARY_PATH to use when resolving paths"
    )]
    ld_library_path: Vec<String>,

    #[arg(
        short,
        long = "scope",
        default_value = "/",
        help = "Only consider libraries under these paths"
    )]
    scopes: Vec<String>,

    #[arg(long="stdout", default_value_t, ignore_case = true, value_parser = clap_enum_variants!(StdoutFormat), help="The format to use when printing to stdout")]
    stdout_format: StdoutFormat,

    #[arg(long = "save-json-to", help = "Save the json to a file")]
    save_json_to: Option<String>,

    #[arg(long = "pretty-json", help = "Pretty print the json")]
    pretty_json: bool,

    #[arg(
        long,
        default_value = "1",
        help = "The number of highest required glibc versions to print"
    )]
    versions: usize,

    #[arg(long="detail-level", default_value_t, ignore_case = true, value_parser = clap_enum_variants!(DetailLevel), help="The detail level to use when printing to stdout")]
    detail_level: DetailLevel,

    #[arg(long="print-error", default_value_t, ignore_case = true, value_parser = clap_enum_variants!(PrintError), help="If and what errors to print to stderr")]
    print_error: PrintError,
}

fn main() -> Result<(), Box<dyn Error>> {
    let parsed_args = Args::parse();
    let mut wants: HashMap<String, HashMap<String, HashSet<PathBuf>>> = HashMap::new();
    let mut errored: HashMap<PathBuf, (String, HashSet<String>)> = HashMap::new();

    let root = PathBuf::try_from(parsed_args.root).unwrap_or(PathBuf::from("/"));
    let lib_paths: Vec<_> = parsed_args
        .ld_library_path
        .iter()
        .map(|s| PathBuf::from(s))
        .collect::<Vec<PathBuf>>();
    let scopes: Vec<_> = parsed_args
        .scopes
        .iter()
        .map(|s| PathBuf::from(s))
        .collect::<Vec<PathBuf>>();
    let analyzer = DependencyAnalyzer::new(root.clone()).library_paths(lib_paths.clone());
    let mut visited = HashSet::new();

    for pathname in &parsed_args.paths {
        let deps = analyzer.clone().analyze(&pathname)?;
        for needed in deps.needed {
            gather_deps_required_libc_version(
                &pathname,
                &needed,
                &deps.libraries,
                &scopes,
                &mut wants,
                &mut visited,
                &mut errored,
            );
        }
    }

    let wants_json = if parsed_args.detail_level.is_version() {
        let mut versions = wants.keys().collect::<Vec<&String>>();
        versions.sort();
        versions.reverse();
        let user_wants = versions
            .iter()
            .take(parsed_args.versions)
            .map(|x| *x)
            .collect::<Vec<&String>>();

        if parsed_args.stdout_format.is_text() {
            for version in &user_wants {
                println!("{}", version);
            }
        }

        if parsed_args.pretty_json {
            Some(serde_json::to_string_pretty(&user_wants)?)
        } else {
            Some(serde_json::to_string(&user_wants)?)
        }
    } else if parsed_args.detail_level.is_function() {
        let mut user_wants: HashMap<String, HashSet<String>> = HashMap::new();
        let mut versions = wants.keys().collect::<Vec<&String>>();
        versions.sort();
        versions.reverse();
        let versions = versions
            .iter()
            .take(parsed_args.versions)
            .map(|x| *x)
            .collect::<Vec<&String>>();

        for version in versions {
            user_wants.insert(
                version.to_string(),
                wants
                    .get(version)
                    .unwrap()
                    .keys()
                    .map(|x| x.to_string())
                    .collect::<HashSet<String>>(),
            );
        }

        if parsed_args.stdout_format.is_text() {
            for (version, functions) in &user_wants {
                for function in functions {
                    println!("{} => {}", version, function);
                }
            }
        }

        if parsed_args.pretty_json {
            Some(serde_json::to_string_pretty(&user_wants)?)
        } else {
            Some(serde_json::to_string(&user_wants)?)
        }
    } else if parsed_args.detail_level.is_file() {
        let mut user_wants: HashMap<String, HashMap<String, HashSet<PathBuf>>> = HashMap::new();
        let mut versions = wants.keys().collect::<Vec<&String>>();
        versions.sort();
        versions.reverse();
        let versions = versions
            .iter()
            .take(parsed_args.versions)
            .map(|x| *x)
            .collect::<Vec<&String>>();

        for version in versions {
            user_wants.insert(version.to_string(), wants.get(version).unwrap().clone());
        }

        if parsed_args.stdout_format.is_text() {
            for (version, functions) in &user_wants {
                for (function, files) in functions {
                    for file in files {
                        println!("{} => {} => {}", version, function, file.display());
                    }
                }
            }
        }

        if parsed_args.pretty_json {
            Some(serde_json::to_string_pretty(&user_wants)?)
        } else {
            Some(serde_json::to_string(&user_wants)?)
        }
    } else {
        None
    };

    if let Some(json) = wants_json {
        if parsed_args.save_json_to.is_some() {
            std::fs::write(parsed_args.save_json_to.unwrap(), &json)?;
        }
        if parsed_args.stdout_format.is_json() {
            println!("{}", json);
        }
    }

    match parsed_args.print_error {
        PrintError::All => {
            for (path, (error, names)) in &errored {
                for name in names {
                    eprintln!(
                        "file={}, reason={}, referenced_by={}",
                        path.display(),
                        error,
                        name
                    );
                }
            }
        }
        PrintError::CannotParse => {
            for (path, (error, names)) in &errored {
                if error == "cannot_parse" {
                    for name in names {
                        eprintln!("{} => {} => {}", path.display(), error, name);
                    }
                }
            }
        }
        PrintError::CannotRead => {
            for (path, (error, names)) in &errored {
                if error == "cannot_read" {
                    for name in names {
                        eprintln!("{} => {} => {}", path.display(), error, name);
                    }
                }
            }
        }
        PrintError::NotFound => {
            for (path, (error, names)) in &errored {
                if error == "not_found" {
                    for name in names {
                        eprintln!("{} => {} => {}", path.display(), error, name);
                    }
                }
            }
        }
        PrintError::None => {}
    }

    if !errored.is_empty() {
        if parsed_args
            .paths
            .iter()
            .find(|x| errored.contains_key(&PathBuf::from(x)))
            .is_some()
        {
            std::process::exit(1);
        }
    }
    Ok(())
}

fn find_required_glibc_version<'a, 'b>(
    referenced_by: &str,
    tab: &ParsingTable<'a, AnyEndian, Symbol>,
    str: &StringTable<'b>,
    from_file: &PathBuf,
    map: &mut HashMap<String, HashMap<String, HashSet<PathBuf>>>,
    errored: &mut HashMap<PathBuf, (String, HashSet<String>)>,
) {
    for sym in tab.iter() {
        if let Ok(name) = str.get(sym.st_name as usize) {
            if !name.is_empty() {
                if name.contains("@@GLIBC_") {
                    let parsed = name.split("@@GLIBC_").collect::<Vec<&str>>();
                    if parsed.len() != 2 {
                        // todo: error?
                        continue;
                    }
                    let function_name = parsed[0];
                    let wants = parsed[1];
                    let v = map.entry(wants.to_string()).or_insert(HashMap::new());
                    let v = v.entry(function_name.to_string()).or_insert(HashSet::new());
                    v.insert(from_file.clone());
                }
            }
        } else {
            errored
                .entry(from_file.clone())
                .or_insert(("".to_string(), HashSet::new()))
                .1
                .insert(referenced_by.to_string());
        }
    }
}

fn gather_deps_required_libc_version(
    referenced_by: &str,
    name: &str,
    libraries: &HashMap<String, Library>,
    scopes: &Vec<PathBuf>,
    wants: &mut HashMap<String, HashMap<String, HashSet<PathBuf>>>,
    visited: &mut HashSet<PathBuf>,
    errored: &mut HashMap<PathBuf, (String, HashSet<String>)>,
) {
    let mut paths = HashSet::new();
    gather_deps_paths(
        referenced_by,
        name,
        libraries,
        scopes,
        &mut paths,
        visited,
        errored,
    );
    for lib_path in paths {
        if let Ok(file_data) = std::fs::read(lib_path.clone()) {
            let slice = file_data.as_slice();
            if let Ok(file) = ElfBytes::<AnyEndian>::minimal_parse(slice) {
                if let Ok(common) = file.find_common_data() {
                    if let (Some(dynsym), Some(dynstr)) = (common.dynsyms, common.dynsyms_strs) {
                        find_required_glibc_version(
                            &referenced_by,
                            &dynsym,
                            &dynstr,
                            &lib_path,
                            wants,
                            errored,
                        );
                    }
                    if let (Some(symtab), Some(strtab)) = (common.symtab, common.symtab_strs) {
                        find_required_glibc_version(
                            &referenced_by,
                            &symtab,
                            &strtab,
                            &lib_path,
                            wants,
                            errored,
                        );
                    }
                }
            } else {
                errored
                    .entry(lib_path.clone())
                    .or_insert(("cannot_parse".to_string(), HashSet::new()))
                    .1
                    .insert(name.to_string());
            }
        } else {
            errored
                .entry(lib_path.clone())
                .or_insert(("cannot_read".to_string(), HashSet::new()))
                .1
                .insert(name.to_string());
        }
    }
}

fn gather_deps_paths<'a>(
    referenced_by: &'a str,
    name: &'a str,
    libraries: &'a HashMap<String, Library>,
    scopes: &Vec<PathBuf>,
    paths: &mut HashSet<PathBuf>,
    visited: &mut HashSet<PathBuf>,
    errored: &mut HashMap<PathBuf, (String, HashSet<String>)>,
) {
    if let Some(lib) = libraries.get(name) {
        if let Some(path) = lib.realpath.as_ref() {
            if let Some(_scope) = scopes.iter().find(|scope| path.starts_with(scope)) {
                if !paths.insert(path.to_path_buf()) || !visited.insert(path.to_path_buf()) {
                    return;
                };
            }
        } else {
            errored
                .entry(lib.path.clone())
                .or_insert(("not_found".to_string(), HashSet::new()))
                .1
                .insert(referenced_by.to_string());
        }

        for needed in &lib.needed {
            gather_deps_paths(
                referenced_by,
                &needed,
                libraries,
                scopes,
                paths,
                visited,
                errored,
            );
        }
    }
}
