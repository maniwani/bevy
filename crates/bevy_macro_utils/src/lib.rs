#![allow(clippy::type_complexity)]

extern crate proc_macro;

mod attrs;
mod shape;
mod symbol;

pub use attrs::*;
pub use shape::*;
pub use symbol::*;

use proc_macro::{TokenStream, TokenTree};
use quote::quote;
use rustc_hash::FxHashSet;
use std::{env, path::PathBuf};
use syn::Ident;
use toml_edit::{Document, Item};

pub struct BevyManifest {
    manifest: Document,
}

impl Default for BevyManifest {
    fn default() -> Self {
        Self {
            manifest: env::var_os("CARGO_MANIFEST_DIR")
                .map(PathBuf::from)
                .map(|mut path| {
                    path.push("Cargo.toml");
                    if !path.exists() {
                        panic!(
                            "No Cargo manifest found for crate. Expected: {}",
                            path.display()
                        );
                    }
                    let manifest = std::fs::read_to_string(path.clone()).unwrap_or_else(|_| {
                        panic!("Unable to read cargo manifest: {}", path.display())
                    });
                    manifest.parse::<Document>().unwrap_or_else(|_| {
                        panic!("Failed to parse cargo manifest: {}", path.display())
                    })
                })
                .expect("CARGO_MANIFEST_DIR is not defined."),
        }
    }
}
const BEVY: &str = "bevy";
const BEVY_INTERNAL: &str = "bevy_internal";

impl BevyManifest {
    pub fn maybe_get_path(&self, name: &str) -> Option<syn::Path> {
        fn dep_package(dep: &Item) -> Option<&str> {
            if dep.as_str().is_some() {
                None
            } else {
                dep.get("package").map(|name| name.as_str().unwrap())
            }
        }

        let find_in_deps = |deps: &Item| -> Option<syn::Path> {
            let package = if let Some(dep) = deps.get(name) {
                return Some(Self::parse_str(dep_package(dep).unwrap_or(name)));
            } else if let Some(dep) = deps.get(BEVY) {
                dep_package(dep).unwrap_or(BEVY)
            } else if let Some(dep) = deps.get(BEVY_INTERNAL) {
                dep_package(dep).unwrap_or(BEVY_INTERNAL)
            } else {
                return None;
            };

            let mut path = Self::parse_str::<syn::Path>(package);
            if let Some(module) = name.strip_prefix("bevy_") {
                path.segments.push(Self::parse_str(module));
            }
            Some(path)
        };

        let deps = self.manifest.get("dependencies");
        let deps_dev = self.manifest.get("dev-dependencies");

        deps.and_then(find_in_deps)
            .or_else(|| deps_dev.and_then(find_in_deps))
    }

    /// Returns the path for the crate with the given name.
    ///
    /// This is a convenience method for constructing a [manifest] and
    /// calling the [`get_path`] method.
    ///
    /// This method should only be used where you just need the path and can't
    /// cache the [manifest]. If caching is possible, it's recommended to create
    /// the [manifest] yourself and use the [`get_path`] method.
    ///
    /// [`get_path`]: Self::get_path
    /// [manifest]: Self
    pub fn get_path_direct(name: &str) -> syn::Path {
        Self::default().get_path(name)
    }

    pub fn get_path(&self, name: &str) -> syn::Path {
        self.maybe_get_path(name)
            .unwrap_or_else(|| Self::parse_str(name))
    }

    pub fn parse_str<T: syn::parse::Parse>(path: &str) -> T {
        syn::parse(path.parse::<TokenStream>().unwrap()).unwrap()
    }

    pub fn get_subcrate(&self, subcrate: &str) -> Option<syn::Path> {
        self.maybe_get_path(BEVY)
            .map(|bevy_path| {
                let mut segments = bevy_path.segments;
                segments.push(BevyManifest::parse_str(subcrate));
                syn::Path {
                    leading_colon: None,
                    segments,
                }
            })
            .or_else(|| self.maybe_get_path(&format!("bevy_{subcrate}")))
    }
}

/// Finds an identifier that will not conflict with the specified set of tokens.
/// If the identifier is present in `haystack`, extra characters will be added
/// to it until it no longer conflicts with anything.
///
/// Note that the returned identifier can still conflict in niche cases,
/// such as if an identifier in `haystack` is hidden behind an un-expanded macro.
pub fn ensure_no_collision(value: Ident, haystack: TokenStream) -> Ident {
    // Collect all the identifiers in `haystack` into a set.
    let idents = {
        // List of token streams that will be visited in future loop iterations.
        let mut unvisited = vec![haystack];
        // Identifiers we have found while searching tokens.
        let mut found = FxHashSet::default();
        while let Some(tokens) = unvisited.pop() {
            for t in tokens {
                match t {
                    // Collect any identifiers we encounter.
                    TokenTree::Ident(ident) => {
                        found.insert(ident.to_string());
                    }
                    // Queue up nested token streams to be visited in a future loop iteration.
                    TokenTree::Group(g) => unvisited.push(g.stream()),
                    TokenTree::Punct(_) | TokenTree::Literal(_) => {}
                }
            }
        }

        found
    };

    let span = value.span();

    // If there's a collision, add more characters to the identifier
    // until it doesn't collide with anything anymore.
    let mut value = value.to_string();
    while idents.contains(&value) {
        value.push('X');
    }

    Ident::new(&value, span)
}

/// Derive a label trait
///
/// # Args
///
/// - `input`: The [`syn::DeriveInput`] for struct that is deriving the label trait
/// - `trait_path`: The path [`syn::Path`] to the label trait
pub fn derive_label(input: syn::DeriveInput, trait_path: &syn::Path) -> TokenStream {
    let ident = input.ident;
    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();
    let mut where_clause = where_clause.cloned().unwrap_or_else(|| syn::WhereClause {
        where_token: Default::default(),
        predicates: Default::default(),
    });
    where_clause.predicates.push(
        syn::parse2(quote! {
            Self: 'static + Send + Sync + Clone + Eq + ::std::fmt::Debug + ::std::hash::Hash
        })
        .unwrap(),
    );

    (quote! {
        impl #impl_generics #trait_path for #ident #ty_generics #where_clause {}

        impl #impl_generics ::std::convert::AsRef<dyn #trait_path> for #ident #ty_generics #where_clause {
            fn as_ref(&self) -> &dyn #trait_path {
                self
            }
        }
    })
    .into()
}
