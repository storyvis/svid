//! Proc-macro derives for the `svid` crate.
//!
//! Re-exported from `svid` as `svid::Svid`, `svid::SvidDomain`, `svid::bridge!`.
//! Generated code references `::svid::*` paths — depending on this crate
//! directly will not work.

use heck::ToSnakeCase;
use proc_macro::TokenStream;
use proc_macro2::{Span, TokenStream as TokenStream2};
use quote::{format_ident, quote};
use syn::{
    parse::{Parse, ParseStream},
    parse_macro_input, Attribute, Data, DeriveInput, Error, Fields, Ident, LitStr, Token, Type,
};

// ============================================================================
// #[derive(Svid)]
// ============================================================================

#[proc_macro_derive(Svid, attributes(svid))]
pub fn derive_svid(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    expand_svid(input)
        .unwrap_or_else(Error::into_compile_error)
        .into()
}

fn expand_svid(input: DeriveInput) -> Result<TokenStream2, Error> {
    let enum_name = &input.ident;
    let data = match &input.data {
        Data::Enum(d) => d,
        _ => {
            return Err(Error::new_spanned(
                &input.ident,
                "Svid can only be derived on enums",
            ))
        }
    };

    if !has_repr_u16(&input.attrs) {
        return Err(Error::new_spanned(
            &input.ident,
            "Svid requires `#[repr(u16)]` on the enum so variant discriminants \
             can be cast to `u16` for the SVID tag field",
        ));
    }

    let registry_name = parse_registry_attr(&input.attrs)?;
    let wide = is_wide(&input.attrs)?;

    let mut variant_idents = Vec::with_capacity(data.variants.len());
    for v in &data.variants {
        if !matches!(v.fields, Fields::Unit) {
            return Err(Error::new_spanned(
                v,
                "Svid variants must be unit variants like `UserId = 1`",
            ));
        }
        variant_idents.push(v.ident.clone());
    }

    let id_blocks: Vec<TokenStream2> = variant_idents
        .iter()
        .map(|v| {
            let marker = format_ident!("{}Marker", v);
            quote_id_block(enum_name, v, &marker, wide)
        })
        .collect();

    let reserved_guards: Vec<TokenStream2> = variant_idents
        .iter()
        .map(|v| {
            let msg = format!(
                "svid: variant `{}::{}` uses tag value {} which is reserved by svid::RANDOM_ID_TAG for SvidGenerator::generate_random()",
                enum_name, v, 4095
            );
            quote! {
                const _: () = {
                    assert!(
                        (#enum_name::#v as u16) < ::svid::RANDOM_ID_TAG,
                        #msg
                    );
                };
            }
        })
        .collect();

    let registry_block = registry_name
        .map(|reg| quote_registry_block(&reg, &variant_idents, wide))
        .unwrap_or_else(TokenStream2::new);

    Ok(quote! {
        #(#reserved_guards)*
        #(#id_blocks)*
        #registry_block
    })
}

fn has_repr_u16(attrs: &[Attribute]) -> bool {
    for attr in attrs {
        if !attr.path().is_ident("repr") {
            continue;
        }
        let mut found = false;
        let _ = attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("u16") {
                found = true;
            }
            Ok(())
        });
        if found {
            return true;
        }
    }
    false
}

fn parse_registry_attr(attrs: &[Attribute]) -> Result<Option<Ident>, Error> {
    let mut registry = None;
    for attr in attrs {
        if !attr.path().is_ident("svid") {
            continue;
        }
        attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("registry") {
                let value = meta.value()?;
                let id: Ident = value.parse()?;
                registry = Some(id);
                Ok(())
            } else if meta.path.is_ident("bits") {
                let _: syn::LitInt = meta.value()?.parse()?;
                Ok(())
            } else {
                Err(meta
                    .error("unknown svid attribute; expected `registry = Ident` or `bits = 128`"))
            }
        })?;
    }
    Ok(registry)
}

fn quote_id_block(enum_name: &Ident, v: &Ident, marker: &Ident, wide: bool) -> TokenStream2 {
    let encode_name = if wide {
        format_ident!("to_uuid")
    } else {
        format_ident!("to_base58")
    };
    let decode_name = if wide {
        format_ident!("from_uuid")
    } else {
        format_ident!("from_base58")
    };
    let raw_decoder = if wide {
        format_ident!("human_readable_to_id")
    } else {
        format_ident!("decode_i64_base58")
    };
    let raw = if wide {
        quote!(::svid::Svid128)
    } else {
        quote!(i64)
    };
    let sql = if wide {
        quote!(::diesel::sql_types::Uuid)
    } else {
        quote!(::diesel::sql_types::BigInt)
    };
    let helpers = if wide {
        quote!(::svid::entity128)
    } else {
        quote!(::svid)
    };
    let hydrate = if wide {
        quote! { fn hydrate_string(value: &str) -> Result<Self, ::autosurgeon::HydrateError> {
            value.parse().map_err(|e: String| ::autosurgeon::HydrateError::unexpected("typed SVID128", e))
        } }
    } else {
        quote! { fn hydrate_int(i: i64) -> Result<Self, ::autosurgeon::HydrateError> { Ok(Self(i)) } }
    };
    let narrow_methods = if wide {
        quote!()
    } else {
        quote! {
            pub fn to_i64(&self) -> i64 { self.0 }
            pub fn to_hex16(&self) -> String { ::svid::id_to_hex16(self.0) }
            pub fn encode_hex16_into<'a>(&self, buf: &'a mut [u8; 16]) -> &'a str {
                ::svid::encode_hex16_into(self.0, buf)
            }
        }
    };
    let qualified_label = format!("{}::{}", enum_name, v);
    quote! {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        #[cfg_attr(feature = "diesel", derive(::diesel::AsExpression, ::diesel::FromSqlRow))]
        #[cfg_attr(feature = "diesel", diesel(sql_type = #sql))]
        #[cfg_attr(feature = "ts", derive(::ts_rs::TS))]
        #[cfg_attr(feature = "ts", ts(export))]
        #[repr(transparent)]
        pub struct #v(#[cfg_attr(feature = "ts", ts(type = "string"))] pub #raw);

        impl ::std::convert::From<#raw> for #v {
            fn from(id: #raw) -> Self { Self(id) }
        }

        impl #v {
            pub fn #encode_name(&self) -> String {
                #helpers::id_to_human_readable(self.0)
            }

            pub fn #decode_name(s: &str) -> ::std::result::Result<Self, String> {
                use ::svid::SvidExt;
                let id_val = #helpers::#raw_decoder(s)?;
                let expected = #enum_name::#v as u16;
                let got = id_val.tag();
                if got != expected {
                    return Err(format!(
                        "Invalid SVID tag: expected {} ({}), got {}",
                        expected, #qualified_label, got
                    ));
                }
                Ok(Self(id_val))
            }

            #[inline]
            pub fn to_str(&self) -> String {
                #helpers::id_to_human_readable(self.0)
            }

            #[inline]
            pub fn from_str_id(s: &str) -> ::std::result::Result<Self, String> {
                #helpers::human_readable_to_id_expecting(s, #enum_name::#v as u16).map(Self)
            }

            #[inline]
            pub fn to_raw(&self) -> #raw { self.0 }
            #narrow_methods

            /// Fixed 11-char base58 (same as `to_str`) into a stack buffer.
            #[inline]
            pub fn encode_into<'a>(&self, buf: &'a mut [u8; #helpers::HUMAN_READABLE_LEN]) -> &'a str {
                #helpers::encode_str_into(self.0, buf)
            }


        }

        impl ::std::fmt::Display for #v {
            fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
                f.write_str(self.encode_into(&mut [0u8; #helpers::HUMAN_READABLE_LEN]))
            }
        }

        ::svid::__svid_impl_http_width!(#v, #helpers::HUMAN_READABLE_LEN);

        impl ::std::str::FromStr for #v {
            type Err = String;
            fn from_str(s: &str) -> ::std::result::Result<Self, Self::Err> {
                if s.len() == #helpers::HUMAN_READABLE_LEN {
                    Self::from_str_id(s)
                } else {
                    Self::#decode_name(s)
                }
            }
        }

        #[cfg(feature = "serde")]
        impl ::serde::Serialize for #v {
            fn serialize<S>(&self, serializer: S) -> ::std::result::Result<S::Ok, S::Error>
            where S: ::serde::Serializer
            {
                serializer.serialize_str(self.encode_into(&mut [0u8; #helpers::HUMAN_READABLE_LEN]))
            }
        }

        #[cfg(feature = "serde")]
        impl<'de> ::serde::Deserialize<'de> for #v {
            fn deserialize<D>(deserializer: D) -> ::std::result::Result<Self, D::Error>
            where D: ::serde::Deserializer<'de>
            {
                let s = <String as ::serde::Deserialize>::deserialize(deserializer)?;
                if s.len() == #helpers::HUMAN_READABLE_LEN {
                    Self::from_str_id(&s).map_err(::serde::de::Error::custom)
                } else {
                    Self::#decode_name(&s).map_err(::serde::de::Error::custom)
                }
            }
        }

        #[cfg(feature = "diesel")]
        impl ::diesel::serialize::ToSql<#sql, ::diesel::pg::Pg> for #v {
            fn to_sql<'b>(
                &'b self,
                out: &mut ::diesel::serialize::Output<'b, '_, ::diesel::pg::Pg>,
            ) -> ::diesel::serialize::Result {
                use ::std::io::Write;
                out.write_all(&self.0.to_be_bytes())?;
                Ok(::diesel::serialize::IsNull::No)
            }
        }

        #[cfg(feature = "diesel")]
        impl ::diesel::deserialize::FromSql<#sql, ::diesel::pg::Pg> for #v {
            fn from_sql(
                bytes: <::diesel::pg::Pg as ::diesel::backend::Backend>::RawValue<'_>,
            ) -> ::diesel::deserialize::Result<Self> {
                let v = <#raw as ::diesel::deserialize::FromSql<
                    #sql,
                    ::diesel::pg::Pg,
                >>::from_sql(bytes)?;
                Ok(Self(v))
            }
        }

        #[cfg(feature = "autosurgeon")]
        impl ::autosurgeon::Reconcile for #v {
            type Key<'a> = ::autosurgeon::reconcile::NoKey;
            fn reconcile<R: ::autosurgeon::Reconciler>(
                &self,
                reconciler: R,
            ) -> ::std::result::Result<(), R::Error> {
                self.0.reconcile(reconciler)
            }
        }

        #[cfg(feature = "autosurgeon")]
        impl ::autosurgeon::Hydrate for #v {
            #hydrate
        }

        #[derive(Debug, Clone, Copy, Default)]
        pub struct #marker;

        impl ::svid::SvidKind for #marker {
            type Id = #v;
            type Raw = #raw;
            const TAG: u16 = #enum_name::#v as u16;
        }
    }
}

fn quote_registry_block(registry: &Ident, variants: &[Ident], wide: bool) -> TokenStream2 {
    let fields: Vec<Ident> = variants
        .iter()
        .map(|v| Ident::new(&v.to_string().to_snake_case(), v.span()))
        .collect();
    let markers: Vec<Ident> = variants
        .iter()
        .map(|v| format_ident!("{}Marker", v))
        .collect();

    let monotonic_constructor = if wide {
        quote! {}
    } else {
        quote! {
                /// Registry whose generators use the process-wide monotonic
                /// sequencer (collision-free in-process, same ID format).
                pub fn new_monotonic(is_client: bool) -> Self {
                    Self {
                        #( #fields: ::svid::IdGenerator::new_monotonic(is_client), )*
                    }
                }

        }
    };

    quote! {
        #[cfg(not(target_arch = "wasm32"))]
        pub struct #registry {
            #( pub #fields: ::svid::IdGenerator<#markers>, )*
        }

        #[cfg(not(target_arch = "wasm32"))]
        impl #registry {
            pub fn new(is_client: bool) -> Self {
                Self {
                    #( #fields: ::svid::IdGenerator::new(is_client), )*
                }
            }

            #monotonic_constructor

            #[inline]
            pub fn generate_id<T>(&self) -> T
            where
                Self: ::svid::GenerateId<T>,
            {
                <Self as ::svid::GenerateId<T>>::generate(self)
            }
        }

        #(
            #[cfg(not(target_arch = "wasm32"))]
            impl ::svid::GenerateId<#variants> for #registry {
                #[inline]
                fn generate(&self) -> #variants {
                    self.#fields.generate_id()
                }
            }
        )*
    }
}

// ============================================================================
// #[derive(SvidDomain)]
// ============================================================================

#[proc_macro_derive(SvidDomain, attributes(svid))]
pub fn derive_svid_domain(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    expand_svid_domain(input)
        .unwrap_or_else(Error::into_compile_error)
        .into()
}

fn expand_svid_domain(input: DeriveInput) -> Result<TokenStream2, Error> {
    let enum_name = &input.ident;
    let wide = is_wide(&input.attrs)?;
    let encode_name = if wide {
        format_ident!("to_uuid")
    } else {
        format_ident!("to_base58")
    };
    let decode_name = if wide {
        format_ident!("from_uuid")
    } else {
        format_ident!("from_base58")
    };
    let raw_decoder = if wide {
        format_ident!("human_readable_to_id")
    } else {
        format_ident!("decode_i64_base58")
    };
    let raw = if wide {
        quote!(::svid::Svid128)
    } else {
        quote!(i64)
    };
    let sql = if wide {
        quote!(::diesel::sql_types::Uuid)
    } else {
        quote!(::diesel::sql_types::BigInt)
    };
    let helpers = if wide {
        quote!(::svid::entity128)
    } else {
        quote!(::svid)
    };
    let validate = if wide {
        quote!()
    } else {
        quote! {
            if id < 0 { return Err("invalid SVID: sign bit (bit 63) must be 0".to_string()); }
        }
    };
    let narrow_methods = if wide {
        quote!()
    } else {
        quote! {
            pub fn to_i64(&self) -> i64 { self.to_raw() }
            pub fn from_i64(id: i64) -> Result<Self, String> { Self::from_raw(id) }
        }
    };
    let hydrate = if wide {
        quote! {
            fn hydrate_string(value: &str) -> Result<Self, ::autosurgeon::HydrateError> {
                value.parse().map_err(|e: String| ::autosurgeon::HydrateError::unexpected("typed SVID128 domain", e))
            }
        }
    } else {
        quote! {
            fn hydrate_int(i: i64) -> Result<Self, ::autosurgeon::HydrateError> {
                Self::from_raw(i).map_err(|e| ::autosurgeon::HydrateError::unexpected("typed SVID64 domain", e))
            }
        }
    };
    let data = match &input.data {
        Data::Enum(d) => d,
        _ => {
            return Err(Error::new_spanned(
                &input.ident,
                "SvidDomain can only be derived on enums",
            ))
        }
    };

    let (error_label, tag_enum_override) = parse_svid_domain_attrs(&input.attrs)?;
    let tag_enum = tag_enum_override.unwrap_or_else(|| Ident::new("SvidTag", Span::call_site()));

    let mut variants_info: Vec<(Ident, Ident)> = Vec::with_capacity(data.variants.len());
    let mut seen_inner: std::collections::HashMap<String, Ident> = std::collections::HashMap::new();
    for v in &data.variants {
        let inner = extract_single_ident_field(&v.fields)?;
        if let Some(prev) = seen_inner.get(&inner.to_string()) {
            return Err(Error::new_spanned(
                &inner,
                format!(
                    "duplicate inner type `{}` — SvidDomain emits `From<{}> for {}`, which would conflict with the impl for the earlier variant at `{}`",
                    inner, inner, enum_name, prev,
                ),
            ));
        }
        seen_inner.insert(inner.to_string(), inner.clone());
        variants_info.push((v.ident.clone(), inner));
    }

    let variant_idents: Vec<&Ident> = variants_info.iter().map(|(vi, _)| vi).collect();
    let inner_types: Vec<&Ident> = variants_info.iter().map(|(_, it)| it).collect();

    // Repeated separately because quote! repetition requires each `#var` in a
    // group to have the same number of iterations.
    let v1 = variant_idents.clone();
    let v2 = variant_idents.clone();
    let v3 = variant_idents.clone();
    let v4 = variant_idents.clone();
    let v5 = variant_idents.clone();
    let t1 = inner_types.clone();
    let t2 = inner_types.clone();
    let t3 = inner_types.clone();
    let t4 = inner_types.clone();
    let t5 = inner_types.clone();
    let t6 = inner_types.clone();
    let t7 = inner_types.clone();

    Ok(quote! {
        impl #enum_name {
            #narrow_methods
            pub fn tag(&self) -> u16 {
                match self {
                    #( #enum_name::#v1(_) => #tag_enum::#t1 as u16, )*
                }
            }

            pub fn to_raw(&self) -> #raw {
                match self {
                    #( #enum_name::#v2(id) => id.0, )*
                }
            }

            pub fn #encode_name(&self) -> String {
                match self {
                    #( #enum_name::#v3(id) => id.#encode_name(), )*
                }
            }

            pub fn from_raw(id: #raw) -> ::std::result::Result<Self, String> {
                #validate
                use ::svid::SvidExt;
                let tag = id.tag();
                #(
                    if tag == #tag_enum::#t2 as u16 {
                        return Ok(#enum_name::#v4(#t3(id)));
                    }
                )*
                Err(format!(concat!("Invalid ", #error_label, " tag: {}"), tag))
            }

            pub fn #decode_name(s: &str) -> ::std::result::Result<Self, String> {
                Self::from_raw(#helpers::#raw_decoder(s)?)
            }

            #[inline]
            pub fn to_str(&self) -> String {
                #helpers::id_to_human_readable(self.to_raw())
            }

            #[inline]
            pub fn encode_into<'a>(&self, buf: &'a mut [u8; #helpers::HUMAN_READABLE_LEN]) -> &'a str {
                #helpers::encode_str_into(self.to_raw(), buf)
            }

            pub fn from_str_id(s: &str) -> ::std::result::Result<Self, String> {
                let id_val = #helpers::human_readable_to_id(s)?;
                Self::from_raw(id_val)
            }
        }

        impl ::std::fmt::Display for #enum_name {
            fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
                f.write_str(self.encode_into(&mut [0u8; #helpers::HUMAN_READABLE_LEN]))
            }
        }

        ::svid::__svid_impl_http_width!(#enum_name, #helpers::HUMAN_READABLE_LEN);

        impl ::std::str::FromStr for #enum_name {
            type Err = String;
            fn from_str(s: &str) -> ::std::result::Result<Self, Self::Err> {
                if s.len() == #helpers::HUMAN_READABLE_LEN {
                    Self::from_str_id(s)
                } else {
                    Self::#decode_name(s)
                }
            }
        }

        #[cfg(feature = "serde")]
        impl ::serde::Serialize for #enum_name {
            fn serialize<S>(&self, serializer: S) -> ::std::result::Result<S::Ok, S::Error>
            where S: ::serde::Serializer
            {
                serializer.serialize_str(self.encode_into(&mut [0u8; #helpers::HUMAN_READABLE_LEN]))
            }
        }

        #[cfg(feature = "serde")]
        impl<'de> ::serde::Deserialize<'de> for #enum_name {
            fn deserialize<D>(deserializer: D) -> ::std::result::Result<Self, D::Error>
            where D: ::serde::Deserializer<'de>
            {
                use ::std::str::FromStr;
                let s = <String as ::serde::Deserialize>::deserialize(deserializer)?;
                Self::from_str(&s).map_err(::serde::de::Error::custom)
            }
        }

        #(
            impl ::std::convert::From<#t4> for #enum_name {
                fn from(id: #t5) -> Self { #enum_name::#v5(id) }
            }

            impl ::std::convert::TryFrom<#enum_name> for #t6 {
                type Error = String;
                fn try_from(val: #enum_name) -> ::std::result::Result<Self, Self::Error> {
                    #[allow(unreachable_patterns)]
                    match val {
                        #enum_name::#variant_idents(id) => Ok(id),
                        _ => Err(format!(
                            "Expected tag for {} ({}), got tag {}",
                            stringify!(#t7),
                            #tag_enum::#inner_types as u16,
                            val.tag(),
                        )),
                    }
                }
            }
        )*

        impl ::std::convert::TryFrom<#raw> for #enum_name {
            type Error = String;
            fn try_from(id: #raw) -> ::std::result::Result<Self, Self::Error> {
                Self::from_raw(id)
            }
        }

        impl ::std::convert::From<#enum_name> for #raw {
            fn from(val: #enum_name) -> Self { val.to_raw() }
        }

        #[cfg(feature = "diesel")]
        impl ::diesel::serialize::ToSql<#sql, ::diesel::pg::Pg> for #enum_name {
            fn to_sql<'b>(
                &'b self,
                out: &mut ::diesel::serialize::Output<'b, '_, ::diesel::pg::Pg>,
            ) -> ::diesel::serialize::Result {
                use ::std::io::Write;
                out.write_all(&self.to_raw().to_be_bytes())?;
                Ok(::diesel::serialize::IsNull::No)
            }
        }

        #[cfg(feature = "diesel")]
        impl ::diesel::deserialize::FromSql<#sql, ::diesel::pg::Pg> for #enum_name {
            fn from_sql(
                bytes: <::diesel::pg::Pg as ::diesel::backend::Backend>::RawValue<'_>,
            ) -> ::diesel::deserialize::Result<Self> {
                let v = <#raw as ::diesel::deserialize::FromSql<
                    #sql,
                    ::diesel::pg::Pg,
                >>::from_sql(bytes)?;
                <Self as ::std::convert::TryFrom<#raw>>::try_from(v)
                    .map_err(|e: String| e.into())
            }
        }

        #[cfg(feature = "autosurgeon")]
        impl ::autosurgeon::Reconcile for #enum_name {
            type Key<'a> = ::autosurgeon::reconcile::NoKey;
            fn reconcile<R: ::autosurgeon::Reconciler>(
                &self,
                reconciler: R,
            ) -> ::std::result::Result<(), R::Error> {
                self.to_raw().reconcile(reconciler)
            }
        }

        #[cfg(feature = "autosurgeon")]
        impl ::autosurgeon::Hydrate for #enum_name {
            #hydrate
        }
    })
}

fn parse_svid_domain_attrs(attrs: &[Attribute]) -> Result<(LitStr, Option<Ident>), Error> {
    let mut label = None;
    let mut tag = None;
    for attr in attrs {
        if !attr.path().is_ident("svid") {
            continue;
        }
        attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("error_label") {
                let value = meta.value()?;
                label = Some(value.parse::<LitStr>()?);
                Ok(())
            } else if meta.path.is_ident("tag") {
                let value = meta.value()?;
                tag = Some(value.parse::<Ident>()?);
                Ok(())
            } else if meta.path.is_ident("bits") {
                let _: syn::LitInt = meta.value()?.parse()?;
                Ok(())
            } else {
                Err(meta.error(
                    "unknown svid attribute; expected `error_label = \"...\"` or `tag = Ident`",
                ))
            }
        })?;
    }
    let label = label.ok_or_else(|| {
        Error::new(
            Span::call_site(),
            "SvidDomain requires `#[svid(error_label = \"...\")]`",
        )
    })?;
    Ok((label, tag))
}

fn extract_single_ident_field(fields: &Fields) -> Result<Ident, Error> {
    const MSG: &str = "SvidDomain variants must be single-field tuple variants whose inner type is a bare ident (e.g. `Folder(FolderId)`)";
    let unnamed = match fields {
        Fields::Unnamed(u) if u.unnamed.len() == 1 => u,
        _ => return Err(Error::new_spanned(fields, MSG)),
    };
    let ty = &unnamed.unnamed[0].ty;
    match ty {
        Type::Path(tp)
            if tp.qself.is_none()
                && tp.path.segments.len() == 1
                && tp.path.segments[0].arguments.is_empty() =>
        {
            Ok(tp.path.segments[0].ident.clone())
        }
        _ => Err(Error::new_spanned(ty, MSG)),
    }
}

// ============================================================================
// svid::bridge!(Src -> Dst { Variant(IdType), ... })
// ============================================================================

struct BridgeInput {
    src: Ident,
    dst: Ident,
    arms: Vec<(Ident, Ident)>,
}

impl Parse for BridgeInput {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let src: Ident = input.parse()?;
        let _: Token![->] = input.parse()?;
        let dst: Ident = input.parse()?;
        let content;
        syn::braced!(content in input);
        let mut arms = Vec::new();
        while !content.is_empty() {
            let variant: Ident = content.parse()?;
            let inner_content;
            syn::parenthesized!(inner_content in content);
            let inner: Ident = inner_content.parse()?;
            arms.push((variant, inner));
            if !content.is_empty() {
                let _: Token![,] = content.parse()?;
            }
        }
        Ok(BridgeInput { src, dst, arms })
    }
}

#[proc_macro]
pub fn bridge(input: TokenStream) -> TokenStream {
    let BridgeInput { src, dst, arms } = parse_macro_input!(input as BridgeInput);
    let variant_idents: Vec<&Ident> = arms.iter().map(|(v, _)| v).collect();
    let inner_types: Vec<&Ident> = arms.iter().map(|(_, t)| t).collect();

    let expanded = quote! {
        impl ::std::convert::From<#src> for #dst {
            fn from(val: #src) -> Self {
                match val {
                    #( #src::#variant_idents(id) => <#dst as ::std::convert::From<#inner_types>>::from(id), )*
                }
            }
        }
    };
    expanded.into()
}

fn is_wide(attrs: &[Attribute]) -> Result<bool, Error> {
    let mut wide = false;
    for attr in attrs.iter().filter(|a| a.path().is_ident("svid")) {
        attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("bits") {
                let bits = meta
                    .value()?
                    .parse::<syn::LitInt>()?
                    .base10_parse::<u16>()?;
                if bits != 64 && bits != 128 {
                    return Err(meta.error("bits must be 64 or 128"));
                }
                wide = bits == 128;
            } else {
                let _: syn::Expr = meta.value()?.parse()?;
            }
            Ok(())
        })?;
    }
    Ok(wide)
}
