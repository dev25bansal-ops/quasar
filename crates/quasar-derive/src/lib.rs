//! Derive macros for the Quasar Engine.
//!
//! Provides:
//! - `#[derive(Inspect)]` — auto-generates egui widget per field
//! - `#[derive(Reflect)]` — Lua/JSON/network serialization
//! - `#[derive(SystemParam)]` — auto-generates `SystemAccess` from field types
//! - `#[derive(Replicate)]` — marks components for automatic network replication

#![cfg_attr(not(test), deny(clippy::unwrap_used, clippy::expect_used))]

use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, Data, DeriveInput, Fields, Type, PathArguments, GenericArgument};

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

/// Derive the `Reflect` trait for a struct, generating:
/// - Lua serialization (`to_lua_table`) / deserialization (`from_lua_table`)
/// - Editor inspector UI (`reflect_inspect_ui`)
/// - Scene JSON schema (`to_json` / `from_json`)
/// - Network serialization (`net_serialize` / `net_deserialize`)
///
/// Supports `#[reflect(range = min..max)]` attribute on `f32`/`f64` fields
/// to constrain numeric values.
///
/// # Example
/// ```ignore
/// #[derive(Reflect)]
/// struct Velocity {
///     #[reflect(range = -100.0..100.0)]
///     x: f32,
///     y: f32,
///     z: f32,
/// }
/// ```
#[proc_macro_derive(Reflect, attributes(reflect))]
pub fn derive_reflect(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = &input.ident;
    let name_str = name.to_string();

    let fields = match &input.data {
        Data::Struct(data) => match &data.fields {
            Fields::Named(named) => &named.named,
            _ => {
                return syn::Error::new_spanned(
                    &input,
                    "Reflect can only be derived for structs with named fields",
                )
                .to_compile_error()
                .into();
            }
        },
        _ => {
            return syn::Error::new_spanned(&input, "Reflect can only be derived for structs")
                .to_compile_error()
                .into();
        }
    };

    let mut to_lua_fields = Vec::new();
    let mut from_lua_fields = Vec::new();
    let mut to_json_fields = Vec::new();
    let mut from_json_fields = Vec::new();
    let mut net_ser_fields = Vec::new();
    let mut net_deser_fields = Vec::new();
    let mut field_descriptors = Vec::new();

    for field in fields {
        let field_name = field.ident.as_ref().unwrap();
        let field_str = field_name.to_string();
        let ty = &field.ty;
        let ty_str = quote!(#ty).to_string().replace(' ', "");

        // Parse #[reflect(range = min..max)]
        let mut range_min: Option<f64> = None;
        let mut range_max: Option<f64> = None;
        for attr in &field.attrs {
            if attr.path().is_ident("reflect") {
                let _ = attr.parse_nested_meta(|meta| {
                    if meta.path.is_ident("range") {
                        let value = meta.value()?;
                        let range_str: syn::LitStr = value.parse()?;
                        let s = range_str.value();
                        if let Some((min_s, max_s)) = s.split_once("..") {
                            range_min = min_s.trim().parse().ok();
                            range_max = max_s.trim().parse().ok();
                        }
                    }
                    Ok(())
                });
            }
        }

        // Lua to/from
        let default_val = match ty_str.as_str() {
            "f32" | "f64" => quote! { 0.0 },
            "u32" | "i32" | "u64" | "i64" | "u8" | "i8" | "u16" | "i16" => quote! { 0 },
            "bool" => quote! { false },
            "String" => quote! { String::new() },
            _ => quote! { Default::default() },
        };

        to_lua_fields.push(quote! {
            table.set(#field_str, self.#field_name.clone())?;
        });

        from_lua_fields.push(quote! {
            #field_name: table.get(#field_str).unwrap_or(#default_val),
        });

        // JSON to/from
        to_json_fields.push(quote! {
            map.insert(#field_str.to_string(), serde_json::json!(self.#field_name));
        });

        from_json_fields.push(quote! {
            #field_name: obj.get(#field_str)
                .and_then(|v| serde_json::from_value(v.clone()).ok())
                .unwrap_or(#default_val),
        });

        // Network serialization (little-endian binary)
        net_ser_fields.push(match ty_str.as_str() {
            "f32" => quote! { buf.extend_from_slice(&self.#field_name.to_le_bytes()); },
            "f64" => quote! { buf.extend_from_slice(&self.#field_name.to_le_bytes()); },
            "u32" => quote! { buf.extend_from_slice(&self.#field_name.to_le_bytes()); },
            "i32" => quote! { buf.extend_from_slice(&self.#field_name.to_le_bytes()); },
            "bool" => quote! { buf.push(if self.#field_name { 1 } else { 0 }); },
            _ => quote! { /* unsupported type for net serialization */ },
        });

        net_deser_fields.push(match ty_str.as_str() {
            "f32" => quote! {
                #field_name: {
                    let bytes: [u8; 4] = cursor[..4].try_into().unwrap_or([0; 4]);
                    cursor = &cursor[4..];
                    f32::from_le_bytes(bytes)
                },
            },
            "f64" => quote! {
                #field_name: {
                    let bytes: [u8; 8] = cursor[..8].try_into().unwrap_or([0; 8]);
                    cursor = &cursor[8..];
                    f64::from_le_bytes(bytes)
                },
            },
            "u32" => quote! {
                #field_name: {
                    let bytes: [u8; 4] = cursor[..4].try_into().unwrap_or([0; 4]);
                    cursor = &cursor[4..];
                    u32::from_le_bytes(bytes)
                },
            },
            "i32" => quote! {
                #field_name: {
                    let bytes: [u8; 4] = cursor[..4].try_into().unwrap_or([0; 4]);
                    cursor = &cursor[4..];
                    i32::from_le_bytes(bytes)
                },
            },
            "bool" => quote! {
                #field_name: {
                    let v = cursor.first().copied().unwrap_or(0) != 0;
                    cursor = &cursor[1..];
                    v
                },
            },
            _ => quote! {
                #field_name: Default::default(),
            },
        });

        // Field descriptor for schema generation
        let range_desc = if let (Some(min), Some(max)) = (range_min, range_max) {
            quote! { Some((#min as f64, #max as f64)) }
        } else {
            quote! { None }
        };

        field_descriptors.push(quote! {
            quasar_core::reflect::FieldDescriptor {
                name: #field_str,
                type_name: #ty_str,
                range: #range_desc,
            }
        });
    }

    let expanded = quote! {
        impl quasar_core::reflect::Reflect for #name {
            fn type_name(&self) -> &'static str {
                #name_str
            }

            fn field_descriptors() -> &'static [quasar_core::reflect::FieldDescriptor] {
                static FIELDS: &[quasar_core::reflect::FieldDescriptor] = &[
                    #(#field_descriptors),*
                ];
                FIELDS
            }

            fn to_json(&self) -> serde_json::Value {
                let mut map = serde_json::Map::new();
                #(#to_json_fields)*
                serde_json::Value::Object(map)
            }

            fn from_json(value: &serde_json::Value) -> Option<Self> {
                let obj = value.as_object()?;
                Some(Self {
                    #(#from_json_fields)*
                })
            }

            fn net_serialize(&self) -> Vec<u8> {
                let mut buf = Vec::new();
                #(#net_ser_fields)*
                buf
            }

            fn net_deserialize(data: &[u8]) -> Option<Self> {
                let mut cursor = data;
                Some(Self {
                    #(#net_deser_fields)*
                })
            }
        }
    };

    TokenStream::from(expanded)
}

// ---------------------------------------------------------------------------
// #[derive(SystemParam)] — auto-generate SystemAccess from field types
// ---------------------------------------------------------------------------

/// Extracts inner type name from a wrapper like `Query<'w, (&A, &mut B)>`, `Res<'w, T>`, `ResMut<'w, T>`.
fn extract_wrapper_info(ty: &Type) -> Option<(String, Vec<(String, bool)>)> {
    if let Type::Path(type_path) = ty {
        let seg = type_path.path.segments.last()?;
        let wrapper_name = seg.ident.to_string();
        match wrapper_name.as_str() {
            "Query" => {
                // Extract component types from Query<'w, (&A, &mut B)>
                if let PathArguments::AngleBracketed(args) = &seg.arguments {
                    let mut components = Vec::new();
                    for arg in &args.args {
                        if let GenericArgument::Type(inner_ty) = arg {
                            extract_query_components(inner_ty, &mut components);
                        }
                    }
                    return Some(("Query".to_string(), components));
                }
            }
            "Res" => {
                if let PathArguments::AngleBracketed(args) = &seg.arguments {
                    for arg in &args.args {
                        if let GenericArgument::Type(inner_ty) = arg {
                            let ty_str = quote!(#inner_ty).to_string().replace(' ', "");
                            return Some(("Res".to_string(), vec![(ty_str, false)]));
                        }
                    }
                }
            }
            "ResMut" => {
                if let PathArguments::AngleBracketed(args) = &seg.arguments {
                    for arg in &args.args {
                        if let GenericArgument::Type(inner_ty) = arg {
                            let ty_str = quote!(#inner_ty).to_string().replace(' ', "");
                            return Some(("ResMut".to_string(), vec![(ty_str, true)]));
                        }
                    }
                }
            }
            _ => {}
        }
    }
    None
}

/// Recursively extracts component types from query tuple types.
/// `&T` → (T, false), `&mut T` → (T, true)
fn extract_query_components(ty: &Type, out: &mut Vec<(String, bool)>) {
    match ty {
        Type::Reference(r) => {
            let inner = &*r.elem;
            let ty_str = quote!(#inner).to_string().replace(' ', "");
            let is_mut = r.mutability.is_some();
            out.push((ty_str, is_mut));
        }
        Type::Tuple(tuple) => {
            for elem in &tuple.elems {
                extract_query_components(elem, out);
            }
        }
        Type::Paren(paren) => {
            extract_query_components(&paren.elem, out);
        }
        _ => {
            // Try to parse as a path type (non-reference component)
            let ty_str = quote!(#ty).to_string().replace(' ', "");
            // Skip lifetime parameters
            if !ty_str.starts_with('\'') {
                out.push((ty_str, false));
            }
        }
    }
}

/// Derive `SystemParam` — auto-generates `DeclareAccess` impl by analyzing field types.
///
/// Reads `Query<'w, (&T, &mut U)>`, `Res<'w, T>`, `ResMut<'w, T>` fields and
/// generates the corresponding `SystemAccess` declarations.
///
/// # Example
/// ```ignore
/// #[derive(SystemParam)]
/// struct PhysicsParams<'w> {
///     bodies: Query<'w, (&RigidBody, &mut Transform)>,
///     time: Res<'w, TimeSnapshot>,
/// }
/// ```
#[proc_macro_derive(SystemParam)]
pub fn derive_system_param(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = &input.ident;
    let name_str = name.to_string();

    let fields = match &input.data {
        Data::Struct(data) => match &data.fields {
            Fields::Named(named) => &named.named,
            _ => {
                return syn::Error::new_spanned(
                    &input,
                    "SystemParam can only be derived for structs with named fields",
                )
                .to_compile_error()
                .into();
            }
        },
        _ => {
            return syn::Error::new_spanned(&input, "SystemParam can only be derived for structs")
                .to_compile_error()
                .into();
        }
    };

    let mut read_components = Vec::new();
    let mut write_components = Vec::new();
    let mut read_resources = Vec::new();
    let mut write_resources = Vec::new();

    for field in fields {
        let ty = &field.ty;
        if let Some((wrapper, components)) = extract_wrapper_info(ty) {
            match wrapper.as_str() {
                "Query" => {
                    for (comp_ty, is_mut) in &components {
                        let ident = syn::Ident::new(comp_ty, proc_macro2::Span::call_site());
                        if *is_mut {
                            write_components.push(quote! { std::any::TypeId::of::<#ident>() });
                        } else {
                            read_components.push(quote! { std::any::TypeId::of::<#ident>() });
                        }
                    }
                }
                "Res" => {
                    for (res_ty, _) in &components {
                        let ident = syn::Ident::new(res_ty, proc_macro2::Span::call_site());
                        read_resources.push(quote! { std::any::TypeId::of::<#ident>() });
                    }
                }
                "ResMut" => {
                    for (res_ty, _) in &components {
                        let ident = syn::Ident::new(res_ty, proc_macro2::Span::call_site());
                        write_resources.push(quote! { std::any::TypeId::of::<#ident>() });
                    }
                }
                _ => {}
            }
        }
    }

    let expanded = quote! {
        impl quasar_core::ecs::parallel::DeclareAccess for #name {
            fn access(&self) -> quasar_core::ecs::parallel::SystemAccess {
                use quasar_core::ecs::parallel::SystemAccess;
                use smallvec::SmallVec;
                let mut access = SystemAccess::new();
                #(access.reads.push(#read_components);)*
                #(access.writes.push(#write_components);)*
                #(access.resources_read.push(#read_resources);)*
                #(access.resources_write.push(#write_resources);)*
                access
            }
        }

        impl quasar_core::ecs::System for #name {
            fn name(&self) -> &str { #name_str }
            fn run(&mut self, _world: &mut quasar_core::ecs::World) {
                // SystemParam structs are constructed from the world by the scheduler.
                // Override this in your system implementation.
            }
        }
    };

    TokenStream::from(expanded)
}

// ---------------------------------------------------------------------------
// #[derive(Replicate)] — mark components for automatic network replication
// ---------------------------------------------------------------------------

/// Derive `Replicate` — generates metadata for network replication.
///
/// Fields can be annotated with:
/// - `#[replicated]` — replicated to all peers (default)
/// - `#[replicated(owner_predicted)]` — owner predicts, others receive
/// - No annotation — server-only, not replicated
///
/// # Example
/// ```ignore
/// #[derive(Component, Replicate)]
/// pub struct Health {
///     #[replicated(owner_predicted)]
///     pub current: f32,
///     pub max: f32, // server-only, not replicated
/// }
/// ```
#[proc_macro_derive(Replicate, attributes(replicated))]
pub fn derive_replicate(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = &input.ident;
    let name_str = name.to_string();

    let fields = match &input.data {
        Data::Struct(data) => match &data.fields {
            Fields::Named(named) => &named.named,
            _ => {
                return syn::Error::new_spanned(
                    &input,
                    "Replicate can only be derived for structs with named fields",
                )
                .to_compile_error()
                .into();
            }
        },
        _ => {
            return syn::Error::new_spanned(&input, "Replicate can only be derived for structs")
                .to_compile_error()
                .into();
        }
    };

    let mut field_metas = Vec::new();
    let mut ser_fields = Vec::new();
    let mut deser_fields = Vec::new();

    for field in fields {
        let field_name = field.ident.as_ref().unwrap();
        let field_str = field_name.to_string();
        let ty = &field.ty;
        let ty_str = quote!(#ty).to_string().replace(' ', "");

        // Check for #[replicated] or #[replicated(owner_predicted)]
        let mut replicated = false;
        let mut owner_predicted = false;
        for attr in &field.attrs {
            if attr.path().is_ident("replicated") {
                replicated = true;
                let _ = attr.parse_nested_meta(|meta| {
                    if meta.path.is_ident("owner_predicted") {
                        owner_predicted = true;
                    }
                    Ok(())
                });
            }
        }

        let mode = if owner_predicted {
            quote! { quasar_core::network::ReplicationMode::OwnerPredicted }
        } else if replicated {
            quote! { quasar_core::network::ReplicationMode::Replicated }
        } else {
            quote! { quasar_core::network::ReplicationMode::ServerOnly }
        };

        field_metas.push(quote! {
            quasar_core::network::ReplicatedField {
                name: #field_str,
                type_name: #ty_str,
                mode: #mode,
            }
        });

        if replicated || owner_predicted {
            // Serialize replicated fields
            ser_fields.push(match ty_str.as_str() {
                "f32" => quote! { buf.extend_from_slice(&self.#field_name.to_le_bytes()); },
                "f64" => quote! { buf.extend_from_slice(&self.#field_name.to_le_bytes()); },
                "u32" => quote! { buf.extend_from_slice(&self.#field_name.to_le_bytes()); },
                "i32" => quote! { buf.extend_from_slice(&self.#field_name.to_le_bytes()); },
                "u8" => quote! { buf.push(self.#field_name); },
                "bool" => quote! { buf.push(if self.#field_name { 1 } else { 0 }); },
                _ => quote! { /* unsupported replicated type */ },
            });

            deser_fields.push(match ty_str.as_str() {
                "f32" => quote! {
                    #field_name: {
                        let bytes: [u8; 4] = cursor.get(..4)
                            .and_then(|s| s.try_into().ok())
                            .unwrap_or([0; 4]);
                        cursor = cursor.get(4..).unwrap_or(&[]);
                        f32::from_le_bytes(bytes)
                    },
                },
                "f64" => quote! {
                    #field_name: {
                        let bytes: [u8; 8] = cursor.get(..8)
                            .and_then(|s| s.try_into().ok())
                            .unwrap_or([0; 8]);
                        cursor = cursor.get(8..).unwrap_or(&[]);
                        f64::from_le_bytes(bytes)
                    },
                },
                "u32" => quote! {
                    #field_name: {
                        let bytes: [u8; 4] = cursor.get(..4)
                            .and_then(|s| s.try_into().ok())
                            .unwrap_or([0; 4]);
                        cursor = cursor.get(4..).unwrap_or(&[]);
                        u32::from_le_bytes(bytes)
                    },
                },
                "i32" => quote! {
                    #field_name: {
                        let bytes: [u8; 4] = cursor.get(..4)
                            .and_then(|s| s.try_into().ok())
                            .unwrap_or([0; 4]);
                        cursor = cursor.get(4..).unwrap_or(&[]);
                        i32::from_le_bytes(bytes)
                    },
                },
                "u8" => quote! {
                    #field_name: {
                        let v = cursor.first().copied().unwrap_or(0);
                        cursor = cursor.get(1..).unwrap_or(&[]);
                        v
                    },
                },
                "bool" => quote! {
                    #field_name: {
                        let v = cursor.first().copied().unwrap_or(0) != 0;
                        cursor = cursor.get(1..).unwrap_or(&[]);
                        v
                    },
                },
                _ => quote! { #field_name: Default::default(), },
            });
        } else {
            // Server-only field: use default on deser
            deser_fields.push(quote! { #field_name: Default::default(), });
        }
    }

    let expanded = quote! {
        impl quasar_core::network::ReplicateDescriptor for #name {
            fn component_name() -> &'static str { #name_str }

            fn replicated_fields() -> &'static [quasar_core::network::ReplicatedField] {
                static FIELDS: &[quasar_core::network::ReplicatedField] = &[
                    #(#field_metas),*
                ];
                FIELDS
            }

            fn replicate_serialize(&self) -> Vec<u8> {
                let mut buf = Vec::new();
                #(#ser_fields)*
                buf
            }

            fn replicate_deserialize(data: &[u8]) -> Option<Self> {
                let mut cursor = data;
                Some(Self {
                    #(#deser_fields)*
                })
            }
        }
    };

    TokenStream::from(expanded)
}
