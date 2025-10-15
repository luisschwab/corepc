// SPDX-License-Identifier: CC0-1.0

//! Provides a data structure that describes each JSON RPC method.

pub mod v17;
pub mod v18;
pub mod v19;
pub mod v20;
pub mod v21;
pub mod v22;
pub mod v23;
pub mod v24;
pub mod v25;
pub mod v26;
pub mod v27;
pub mod v28;
pub mod v29;
pub mod v30;

use crate::Version;

/// Returns a list of all the method names.
pub fn all_methods(version: Version) -> Vec<String> {
    use Version::*;

    let list = match version {
        V17 => v17::METHODS,
        V18 => v18::METHODS,
        V19 => v19::METHODS,
        V20 => v20::METHODS,
        V21 => v21::METHODS,
        V22 => v22::METHODS,
        V23 => v23::METHODS,
        V24 => v24::METHODS,
        V25 => v25::METHODS,
        V26 => v26::METHODS,
        V27 => v27::METHODS,
        V28 => v28::METHODS,
        V29 => v29::METHODS,
        V30 => v30::METHODS,
    };

    list.iter().map(|m| m.name.to_string()).collect()
}

/// Describes a single JSON RPC method.
// TODO: This name is overloaded (`versioned::Method`).
#[derive(Debug)]
pub struct Method {
    /// The method name.
    pub name: &'static str,
    /// The type it is expected to return (method name in snake case).
    ///
    /// `None` if the method is either intentionally omitted or not yet done.
    pub ret: Option<Return>,
    /// `true` if this method requires a type to exist in `model`.
    ///
    /// This is true if the return type includes types that can be
    /// more strongly typed using `rust-bitcoin`.
    pub requires_model: bool,
    /// The function name (snake case) for this method.
    pub function: &'static str,
}

impl Method {
    /// Returns a `Method` type if one exists in the `METHODS` list for `name`.
    pub fn from_name(version: Version, name: &str) -> Option<&'static Method> {
        use Version::*;
        let list = match version {
            V17 => v17::METHODS,
            V18 => v18::METHODS,
            V19 => v19::METHODS,
            V20 => v20::METHODS,
            V21 => v21::METHODS,
            V22 => v22::METHODS,
            V23 => v23::METHODS,
            V24 => v24::METHODS,
            V25 => v25::METHODS,
            V26 => v26::METHODS,
            V27 => v27::METHODS,
            V28 => v28::METHODS,
            V29 => v29::METHODS,
            V30 => v30::METHODS,
        };

        list.iter().find(|&method| method.name == name)
    }

    /// Represents a `Method` that requires a custom type as well as a type in `model`.
    const fn new_modelled(name: &'static str, ty: &'static str, function: &'static str) -> Method {
        Method { name, ret: Some(Return::Type(ty)), requires_model: true, function }
    }

    /// Represents a method that requires a custom type as but no type in `model`.
    ///
    /// Implies this method does not return data that can be strongly type using `rust-bitcoin`.
    const fn new_no_model(name: &'static str, ty: &'static str, function: &'static str) -> Method {
        Method { name, ret: Some(Return::Type(ty)), requires_model: false, function }
    }

    const fn new_nothing(name: &'static str, function: &'static str) -> Method {
        Method { name, ret: Some(Return::Nothing), requires_model: false, function }
    }

    const fn new_numeric(name: &'static str, function: &'static str) -> Method {
        Method { name, ret: Some(Return::Numeric), requires_model: false, function }
    }

    const fn new_bool(name: &'static str, function: &'static str) -> Method {
        Method { name, ret: Some(Return::Bool), requires_model: false, function }
    }

    const fn new_string(name: &'static str, function: &'static str) -> Method {
        Method { name, ret: Some(Return::String), requires_model: false, function }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Return {
    /// Method returns a type (that should exist).
    Type(&'static str),
    /// Method does not return anything.
    Nothing,
    /// Method returns a numeric type.
    Numeric,
    /// Method returns a boolean.
    Bool,
    /// Method returns a string.
    String,
}
