
struct Project {
    name: String,
    readme: String,
    modules: Vec<Module>,
}

struct Module {
    name: String,
    exports: Vec<Export>,
}

enum Export {
    Class(Class),
    Var(Var)
}

struct Class {
    name: String,
    fields: Vec<Var>,
    methods: Vec<Func>,
}

struct Var {
    name: String,
    ty: String,
}

enum Type {
    Simple(String),
    Union(Vec<Type>),
    Func(Func),
}

struct Func {
    name: Option<String>,
    args: Vec<Var>,
    ret: Vec<String>,
}
