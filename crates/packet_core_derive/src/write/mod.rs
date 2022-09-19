use falcon_proc_util::ErrorCatcher;
use proc_macro2::TokenStream;
use quote::ToTokens;
use syn::spanned::Spanned;
use syn::{
    parse_quote, parse_quote_spanned, Error, Expr, Fields, ItemImpl, ItemStruct, Stmt, Token,
};

use crate::util::ParsedFields;

use self::check::validate;
use self::generate::{to_end, to_preprocess, to_tokenstream};

mod check;
mod generate;

pub(crate) fn implement_write(item: ItemStruct) -> syn::Result<TokenStream> {
    let mut error = ErrorCatcher::new();

    match &item.fields {
        Fields::Named(fields) => {
            let fields = error.critical(ParsedFields::new(&fields.named, validate))?;
            return Ok(generate_tokens(&item, fields).into_token_stream());
        }
        _ => error.add_error(Error::new(
            item.fields.span(),
            "Only named fields are supported currently",
        )),
    }

    error.emit()?;
    Ok(TokenStream::new())
}

fn generate_tokens(item: &ItemStruct, parsed: ParsedFields) -> ItemImpl {
    let mut preprocess: Vec<Stmt> = Vec::new();
    let mut writes: Vec<Stmt> = Vec::with_capacity(parsed.fields.len());

    for (field, data) in parsed.fields {
        let ident = &field.ident;
        let field_ty = &field.ty;
        let mut field: Expr = parse_quote_spanned! {field.span()=> self.#ident };

        let mut end = None;

        for (i, attribute) in data.iter().enumerate() {
            field = to_tokenstream(attribute, field, field_ty);
            if i == data.len() - 1 {
                if let Some(process) = to_preprocess(attribute, field.clone()) {
                    preprocess.push(process);
                }
                end = to_end(attribute, field.clone());
            }
        }

        writes.push(end.unwrap_or_else(|| {
            parse_quote_spanned! {field.span()=>
                ::falcon_packet_core::PacketWrite::write(
                    #field,
                    buffer,
                )?;
            }
        }));
    }
    let mutable: Option<Token![mut]> = if preprocess.is_empty() {
        None
    } else {
        Some(parse_quote!(mut))
    };

    let ident = &item.ident;
    let (impl_generics, ty_generics, where_clause) = item.generics.split_for_impl();
    parse_quote_spanned! {item.ident.span()=>
        #[automatically_derived]
        impl #impl_generics ::falcon_packet_core::PacketWrite for #ident #ty_generics #where_clause {
            #[allow(clippy::useless_conversion)]
            fn write<B>(#mutable self, buffer: &mut B) -> ::std::result::Result<(), ::falcon_packet_core::WriteError>
            where
                B: ::bytes::BufMut + ?Sized
            {
                #(#preprocess)*
                #(#writes)*
                Ok(())
            }
        }
    }
}
