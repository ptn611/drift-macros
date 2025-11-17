extern crate proc_macro;

use proc_macro::TokenStream;
use quote::{format_ident, quote};
use syn::{Data, DeriveInput, Fields, Ident, ItemStruct, Type, parse_macro_input, parse_quote};

#[allow(clippy::panic)]
#[proc_macro_attribute]
pub fn assert_no_slop(_: TokenStream, input: TokenStream) -> TokenStream {
    let derive_input = parse_macro_input!(input as DeriveInput);
    let struct_name = &derive_input.ident;

    let struct_name_uppercase = struct_name.to_string().to_uppercase();

    let expanded = match &derive_input.data {
        Data::Struct(data_struct) => match &data_struct.fields {
            Fields::Named(fields) => {
                let field_sizes = fields.named.iter().map(|field| &field.ty);
                let sizes_sum = quote! { #(std::mem::size_of::<#field_sizes>())+* };
                let struct_size_name = format_ident! { "{}_STRUCT_SIZE", struct_name_uppercase };
                let field_sizes_name = format_ident! {  "{}_FIELD_SIZES", struct_name_uppercase };

                quote! {
                    const #struct_size_name : usize = std::mem::size_of::<#struct_name>();
                    const #field_sizes_name : usize = #sizes_sum;

                    const_assert_eq!(#struct_size_name, #field_sizes_name);
                }
            }
            Fields::Unnamed(fields) => {
                let field_types = fields.unnamed.iter().map(|field| &field.ty);
                let sizes_sum = quote! { #(std::mem::size_of::<#field_types>())+* };

                let struct_size_name = format_ident! { "{}_STRUCT_SIZE", struct_name_uppercase };
                let field_sizes_name = format_ident! {  "{}_FIELD_SIZES", struct_name_uppercase };

                quote! {
                    const #struct_size_name : usize = std::mem::size_of::<#struct_name>();
                    const #field_sizes_name : usize = #sizes_sum;

                    const_assert_eq!(#struct_size_name, #field_sizes_name);
                }
            }
            Fields::Unit => {
                panic!("assert_no_padding attribute cannot be used on unit structs");
            }
        },
        _ => {
            panic!("assert_no_padding attribute can only be used on structs");
        }
    };

    let output = quote! {
        #derive_input
        #expanded
    };
    output.into()
}

/// #[legacy_layout]
///
/// Mark a struct as using the legacy u128/i128 layout for stable zero-copy
/// deserialization across compiler versions.
///
/// Usage:
///
///     #[legacy_layout]                  // rewrite fields + generate accessors
///     #[legacy_layout(no_accessors)]    // rewrite fields only
#[proc_macro_attribute]
pub fn legacy_layout(attr: TokenStream, item: TokenStream) -> TokenStream {
    let mut s = parse_macro_input!(item as ItemStruct);

    // ---- Parse attr flags ----
    let attr_args = parse_macro_input!(attr as syn::AttributeArgs);
    let mut disable_accessors = false;

    for arg in attr_args {
        match arg {
            syn::NestedMeta::Meta(syn::Meta::Path(path)) if path.is_ident("no_accessors") => {
                disable_accessors = true;
            }
            _ => {
                return syn::Error::new_spanned(arg, "Unknown attribute argument")
                    .to_compile_error()
                    .into();
            }
        }
    }

    // ---- Rewrite fields ----
    let mut rewritten_fields: Vec<(Ident, bool)> = Vec::new();

    for field in s.fields.iter_mut() {
        let Some(ident) = &field.ident else { continue };

        if let Type::Path(type_path) = &field.ty {
            if let Some(seg) = type_path.path.segments.last() {
                match seg.ident.to_string().as_str() {
                    "u128" => {
                        field.ty = parse_quote!(crate::math::bn::compat::u128);
                        rewritten_fields.push((ident.clone(), false));
                    }
                    "i128" => {
                        field.ty = parse_quote!(crate::math::bn::compat::i128);
                        rewritten_fields.push((ident.clone(), true));
                    }
                    _ => {}
                }
            }
        }
    }

    let struct_ident = &s.ident;
    let (impl_generics, ty_generics, where_clause) = s.generics.split_for_impl();

    // ---- Generate getters + setters unless disabled ----
    let impl_block = if rewritten_fields.is_empty() || disable_accessors {
        quote! {}
    } else {
        let accessors = rewritten_fields.iter().map(|(ident, is_signed)| {
            let getter_name = ident.clone();
            let setter_name = Ident::new(&format!("set_{}", ident), ident.span());

            let ret_ty = if *is_signed {
                quote!(i128)
            } else {
                quote!(u128)
            };

            quote! {
                pub fn #getter_name(&self) -> #ret_ty {
                    self.#ident.into()
                }

                pub fn #setter_name(&mut self, value: #ret_ty) {
                    self.#ident = value.into();
                }
            }
        });

        quote! {
            impl #impl_generics #struct_ident #ty_generics #where_clause {
                #(#accessors)*
            }
        }
    };

    // ---- Final expanded output ----
    TokenStream::from(quote! {
        #s
        #impl_block
    })
}
