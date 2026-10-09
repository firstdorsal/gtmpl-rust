//! `#[derive(Gtmpl)]` for [`gtmpl_value_ng`][gtmpl_value_ng]: it generates the
//! `From<YourStruct> for Value` implementation that turns a struct into a template
//! context for [`gtmpl-ng`][gtmpl-ng].
//!
//! ```rust
//! use gtmpl_derive_ng::Gtmpl;
//! use gtmpl_value_ng::Value;
//!
//! #[derive(Gtmpl)]
//! struct Foo {
//!     bar: u8,
//! }
//!
//! let v: Value = Foo { bar: 23 }.into();
//! ```
//!
//! Every named field is converted with `Into<Value>`, so a field type only has to
//! implement that. The generated code names `::gtmpl_value_ng::Value` by absolute
//! path, so `gtmpl_value_ng` has to be a dependency of the deriving crate too.
//!
//! [gtmpl-ng]: https://github.com/firstdorsal/gtmpl-rust
//! [gtmpl_value_ng]: https://docs.rs/gtmpl_value_ng

use proc_macro::TokenStream;
use quote::quote;
use syn::{Data, DataStruct, DeriveInput, Fields, Ident};

#[proc_macro_derive(Gtmpl)]
pub fn gtmpl_derive_ng(input: TokenStream) -> TokenStream {
    let ast = match syn::parse::<DeriveInput>(input) {
        Ok(ast) => ast,
        Err(error) => return error.to_compile_error().into(),
    };
    match implement(&ast) {
        Ok(tokens) => tokens.into(),
        Err(error) => error.to_compile_error().into(),
    }
}

fn implement(ast: &DeriveInput) -> syn::Result<proc_macro2::TokenStream> {
    let name = &ast.ident;
    let fields = named_fields(ast)?;
    let inserts = fields.iter().map(|field| {
        quote! { m.insert(stringify!(#field).to_owned(), s.#field.into()); }
    });

    Ok(quote! {
        impl From<#name> for ::gtmpl_value_ng::Value {
            fn from(s: #name) -> ::gtmpl_value_ng::Value {
                let mut m: ::std::collections::HashMap<String, ::gtmpl_value_ng::Value> =
                    ::std::collections::HashMap::new();
                #(#inserts)*
                ::gtmpl_value_ng::Value::Object(m)
            }
        }
    })
}

/// A template reads a context by field name, so only named fields can be represented.
/// A tuple struct has none -- deriving on one used to produce an empty object and drop
/// every field silently, so it is refused instead.
fn named_fields(ast: &DeriveInput) -> syn::Result<Vec<&Ident>> {
    match ast.data {
        Data::Struct(DataStruct {
            fields: Fields::Named(ref fields),
            ..
        }) => Ok(fields
            .named
            .iter()
            .filter_map(|f| f.ident.as_ref())
            .collect()),
        Data::Struct(DataStruct {
            fields: Fields::Unit,
            ..
        }) => Ok(Vec::new()),
        Data::Struct(DataStruct {
            fields: Fields::Unnamed(ref fields),
            ..
        }) => Err(syn::Error::new_spanned(
            fields,
            "#[derive(Gtmpl)] needs named fields: a template addresses a context by \
             field name, and a tuple struct has no names to address",
        )),
        Data::Enum(ref data) => Err(syn::Error::new_spanned(
            data.enum_token,
            "#[derive(Gtmpl)] is only defined for structs, not for enums",
        )),
        Data::Union(ref data) => Err(syn::Error::new_spanned(
            data.union_token,
            "#[derive(Gtmpl)] is only defined for structs, not for unions",
        )),
    }
}
