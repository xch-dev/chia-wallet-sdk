use std::fs;

use serde::Deserialize;

#[derive(Deserialize)]
pub(crate) struct Binding {
    pub name: String,
    #[serde(flatten, rename = "type")]
    pub kind: BindingType,
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub(crate) enum BindingType {
    Function {
        args: Vec<FunctionArg>,
        returns: String,
    },
}

#[derive(Deserialize)]
pub(crate) struct FunctionArg {
    pub name: String,
    #[serde(rename = "type")]
    pub ty: String,
}

pub(crate) fn bindings(root: &str) -> Vec<Binding> {
    let mut bindings = Vec::new();

    for entry in fs::read_dir(root).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        let binding = fs::read_to_string(path).unwrap();
        bindings.extend(serde_json::from_str::<Vec<Binding>>(&binding).unwrap());
    }

    bindings
}
