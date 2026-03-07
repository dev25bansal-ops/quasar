//! Derive macros for the Quasar Engine.
//!
//! Currently provides `#[derive(Inspect)]` which auto-generates an
//! [`Inspect`] trait implementation for structs, rendering an egui widget
//! per field based on its type.

use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, Data, DeriveInput, Fields};

/// Derive the `Inspect` trait for a struct.
///
/// Each named field gets an automatic egui widget based on its type:
/// - `f32` → `widget_f32`
/// - `f64` → `widget_f64`
/// - `u32` → `widget_u32`
/// - `i32` → `widget_i32`
/// - `bool` → `widget_bool`
/// - `String` → `widget_string`
/// - `[f32; 3]` → `widget_vec3`
/// - `[f32; 4]` → `widget_color4`
///
/// Fields whose type is not recognised are silently skipped.
///
/// # Example
/// ```ignore
/// #[derive(Inspect)]
/// struct Health {
///     current: f32,
///     max: f32,
///     invincible: bool,
/// }
/// ```
#[proc_macro_derive(Inspect, attributes(inspect))]
pub fn derive_inspect(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = &input.ident;
    let label = name.to_string();

    let fields = match &input.data {
        Data::Struct(data) => match &data.fields {
            Fields::Named(named) => &named.named,
            _ => {
                return syn::Error::new_spanned(
                    &input,
                    "Inspect can only be derived for structs with named fields",
                )
                .to_compile_error()
                .into();
            }
        },
        _ => {
            return syn::Error::new_spanned(&input, "Inspect can only be derived for structs")
                .to_compile_error()
                .into();
        }
    };

    let mut widget_calls = Vec::new();

    for field in fields {
        let field_name = field.ident.as_ref().unwrap();
        let field_label = field_name.to_string();
        let ty = &field.ty;

        // Check for #[inspect(skip)]
        let skip = field.attrs.iter().any(|attr| {
            if attr.path().is_ident("inspect") {
                if let Ok(meta) = attr.parse_args::<syn::Ident>() {
                    return meta == "skip";
                }
            }
            false
        });
        if skip {
            continue;
        }

        let ty_str = quote!(#ty).to_string().replace(' ', "");

        let call = match ty_str.as_str() {
            "f32" => quote! {
                changed |= quasar_editor::reflect::widget_f32(
                    ui, #field_label, &mut self.#field_name,
                    &quasar_editor::reflect::FieldMeta::default(),
                );
            },
            "f64" => quote! {
                changed |= quasar_editor::reflect::widget_f64(
                    ui, #field_label, &mut self.#field_name,
                    &quasar_editor::reflect::FieldMeta::default(),
                );
            },
            "u32" => quote! {
                changed |= quasar_editor::reflect::widget_u32(
                    ui, #field_label, &mut self.#field_name,
                    &quasar_editor::reflect::FieldMeta::default(),
                );
            },
            "i32" => quote! {
                changed |= quasar_editor::reflect::widget_i32(
                    ui, #field_label, &mut self.#field_name,
                    &quasar_editor::reflect::FieldMeta::default(),
                );
            },
            "bool" => quote! {
                changed |= quasar_editor::reflect::widget_bool(
                    ui, #field_label, &mut self.#field_name,
                );
            },
            "String" => quote! {
                changed |= quasar_editor::reflect::widget_string(
                    ui, #field_label, &mut self.#field_name,
                );
            },
            "[f32;3]" => quote! {
                changed |= quasar_editor::reflect::widget_vec3(
                    ui, #field_label, &mut self.#field_name, 0.05,
                );
            },
            "[f32;4]" => quote! {
                changed |= quasar_editor::reflect::widget_color4(
                    ui, #field_label, &mut self.#field_name,
                );
            },
            _ => {
                // Unsupported type — skip silently.
                continue;
            }
        };

        widget_calls.push(call);
    }

    let expanded = quote! {
        impl quasar_editor::reflect::Inspect for #name {
            fn inspect_ui(&mut self, ui: &mut egui::Ui) -> bool {
                let mut changed = false;
                #(#widget_calls)*
                changed
            }

            fn type_label(&self) -> &str {
                #label
            }
        }
    };

    TokenStream::from(expanded)
}
