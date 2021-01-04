use std::path::Path;
use serde::Serialize;
use log::{debug, warn};

pub fn main() {
    pretty_env_logger::try_init().ok();
    let mut args = std::env::args().skip(1);
    let path = args.next().unwrap();
    let t = tera::Tera::new("src/templates/*").unwrap();
    let mut context = tera::Context::new();
    let module = parse_module(path);
    context.insert("project", &module);
    let index = t.render("index.html", &context).unwrap();
    std::fs::write("out_dir/index.html", index).unwrap();
    for sub in &module.modules {
        context.insert("module", sub);
        let rendered = t.render("module.html", &context).unwrap();
        debug!("attempting to write {:?}", sub.name);
        std::fs::write(&format!("out_dir/{}.html", sub.name.to_lowercase()), rendered).unwrap();
    }
}


fn parse_module<P: AsRef<Path>>(path: P) -> Module {
    debug!("parse_module {}", path.as_ref().display());
    let mut ret = Module {
        exports: Default::default(),
        modules: Vec::new(),
        path: None,
        name: path.as_ref().file_name().unwrap().to_string_lossy().to_string()
    };
    let mut root_path = path.as_ref().join("init.lua");
    if !root_path.exists() {
        root_path = path.as_ref().join(&format!("{}.lua", ret.name));
        if !root_path.exists() {
            warn!("Root path doesn't exist at either expected location");
            return ret;
        }
    }
    for ent in std::fs::read_dir(&path).unwrap() {
        let e = ent.unwrap();
        if e.file_type().unwrap().is_dir() {
            ret.modules.push(parse_module(e.path()))
        } else if e.path().extension().map(|s| s.to_str()).flatten().unwrap() == "lua" {
            if e.path() == root_path {
                continue;
            }
            let name = e.path().file_stem().unwrap().to_string_lossy().to_string();
            ret.modules.push(Module {
                exports: Default::default(),
                modules: Vec::new(),
                name,
                path: None,
            })
        }
    }
    ret
}

#[derive(Debug, Serialize)]
pub struct Module {
    name: String,
    modules: Vec<Module>,
    exports: Exports,
    path: Option<String>,
}

#[derive(Debug, Default, Serialize)]
pub struct Exports {
    classes: Vec<Export>,
    functions: Vec<Export>,
    variables: Vec<Export>,
}
#[derive(Debug, Serialize)]
pub struct Export {
    /// The name of the exported item
    name: String,
    /// The html description of the item
    desc: String,
    /// The URI for the item
    path: String,
    /// The Class, Function or Variable info about this item
    info: ExportInfo,
}

#[derive(Debug, Serialize)]
pub enum ExportInfo {
    /// A Table that defines a class of object
    Class {
        methods: Vec<Func>,
        fields: Vec<Var>,
    },
    /// A function
    Func(Func),
    /// A variable bit of data
    Var(Var),
}

#[derive(Debug, Serialize)]
pub struct Func {
    /// The function's name, if a method this will be the
    /// value after the `:`
    name: String,
    /// The list of arguments
    args: Vec<Var>,
    /// The return type, defaults to `any`
    ret: TypeDef,
}

#[derive(Debug, Serialize)]
pub struct Var {
    /// The name of this variable or field
    name: String,
    /// The type provided
    ty: TypeDef,
}

#[derive(Debug, Serialize)]
// #[serde(tag = "kind")]
pub enum TypeDef {
    /// A user defined type ex:
    /// ```lua
    /// @class Car
    /// local Car = {}
    /// ```
    User(Type),
    /// A natural lua type ex: `string`
    BuiltIn(Box<BuiltInType>),
    /// A union of types ex: `string|nil`
    Union(Vec<TypeDef>),
    /// An array of a single type ex: `string[]`
    Array(Box<TypeDef>),
}

#[derive(Debug, Serialize)]
pub enum BuiltInType {
    /// Not a true lua type but
    /// used as a catch all for
    /// undeclared values
    Any,
    Nil,
    Boolean,
    String,
    Number,
    Table(TypeDef, TypeDef),
    Thread,
}

#[derive(Debug, Serialize)]
/// A user defined type
pub struct Type {
    name: String,
    /// If declared in this project
    /// this will be the url for its documentation
    path: Option<String>,
}
