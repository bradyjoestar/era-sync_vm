use proc_macro2::{Span, TokenStream};
use proc_macro_error::abort_call_site;
use quote::quote;
use syn::{
    Ident, parse_macro_input, punctuated::Punctuated, token::Comma, DeriveInput, GenericParam, Generics,
    Type,
};

use crate::new_utils::{
    get_type_path_of_field, get_empty_path_field_allocation_of_type, get_ident_of_field_type, get_type_params_from_generics, has_engine_generic_param,
};

pub(crate) fn derive_select(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let derived_input = parse_macro_input!(input as DeriveInput);
    let DeriveInput {
        ident,
        mut generics,
        data,
        ..
    } = derived_input.clone();

    let mut struct_initializations = TokenStream::new();
    let mut array_selections = TokenStream::new();
    let mut field_selections = TokenStream::new();

    match data {
        syn::Data::Struct(ref struct_data) => match struct_data.fields {
            syn::Fields::Named(ref named_fields) => {
                for field in named_fields.named.iter() {
                    let field_ident = field.ident.clone().expect("should have a field elem ident");
                    let local_field_ident = syn::parse_str::<Ident>(&format!("conditionally_select_new_{}", field_ident)).unwrap();
                    let el_ty = get_ident_of_field_type(&field.ty);
                    match field.ty {
                        Type::Array(ref array_ty) => {
                            match *array_ty.elem {
                                Type::Path(ref _p) => {},
                                _ => abort_call_site!("only array of elements is allowed here"),
                            };

                            let len = &array_ty.len;
                            let ty_path = get_type_path_of_field(&field.ty);
                            let empty = get_empty_path_field_allocation_of_type(&ty_path);
                            let empty_array = quote!{
                                let mut #local_field_ident: #array_ty = vec![#empty; #len].try_into().unwrap();
                            };
                            let array_select = quote! {
                                #empty_array
                                for (c, (this, other)) in #local_field_ident.iter_mut().zip(a.#field_ident.iter().zip(b.#field_ident.iter())){
                                    *c = #el_ty::conditionally_select(cs, flag, &this, &other)?;
                                }
                            };
                            array_selections.extend(array_select);
                        }
                        Type::Path(_) => {
                            let field_select = quote! {
                                let mut #local_field_ident = #el_ty::conditionally_select(cs, flag, &a.#field_ident, &b.#field_ident)?;
                            };
                            field_selections.extend(field_select);
                        }
                        _ => abort_call_site!("only array and path types are allowed"),
                    };

                    let init_field = quote! {
                        #field_ident: #local_field_ident,
                    };

                    struct_initializations.extend(init_field);
                }
            }
            _ => abort_call_site!("only named fields are allowed!"),
        },
        _ => abort_call_site!("only struct types are allowed!"),
    }

    let comma = Comma(Span::call_site());
    let mut function_generic_params = Punctuated::new();

    let engine_generic_param = syn::parse_str::<GenericParam>(&"E: Engine").unwrap();
    let has_engine_param  = has_engine_generic_param(&generics.params, &engine_generic_param);
    if has_engine_param == false {
        generics.params.insert(0, engine_generic_param.clone());
        generics.params.push_punct(comma.clone());
    }

    let type_params_of_allocated_struct = get_type_params_from_generics(&generics, &comma, has_engine_param == false);

    // add CS to func generic params
    let cs_generic_param = syn::parse_str::<GenericParam>(&"CS: ConstraintSystem<E>").unwrap();
    function_generic_params.push(cs_generic_param.clone());
    function_generic_params.push_punct(comma.clone());

    let function_generics = Generics {
        lt_token: Some(syn::token::Lt(Span::call_site())),
        params: function_generic_params,
        gt_token: Some(syn::token::Gt(Span::call_site())),
        where_clause: None,
    };


    let expanded = quote! {
        impl#generics CircuitSelectable<E> for #ident<#type_params_of_allocated_struct>{
            fn conditionally_select#function_generics(cs: &mut CS, flag: &Boolean, a: &Self, b: &Self) -> Result<Self, SynthesisError> {
                if CircuitEq::eq(a, b) {
                    return Ok(a.clone());
                }
                
                use num_traits::Zero;
                use std::convert::TryInto;
                #array_selections
                #field_selections

                Ok(Self {
                    #struct_initializations
                })
            }
        }
    };

    proc_macro::TokenStream::from(expanded)
}
