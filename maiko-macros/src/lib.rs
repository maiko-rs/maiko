//! Procedural macros for the Maiko actor runtime.
//!
//! - `#[derive(Event)]`: Implements `maiko::Event` for your type, preserving generics and bounds.
//! - `#[derive(Label)]`: Implements `maiko::Label` for enums, returning variant names.
//! - `#[derive(SelfRouting)]`: Implements `maiko::Topic for T` (with `type Event = T`) for event-as-topic routing.
//!
//! Usage:
//! ```rust,ignore
//! use maiko::{Event, Label, SelfRouting};
//!
//! // Simple event without topic routing
//! #[derive(Clone, Debug, Event)]
//! enum MyEvent { Foo, Bar }
//!
//! // Event with labels for observability (logging, diagrams)
//! #[derive(Clone, Debug, Event, Label)]
//! enum MyEvent { Foo, Bar }
//!
//! // Event that routes itself (event-as-topic pattern)
//! #[derive(Clone, Debug, Hash, PartialEq, Eq, Event, SelfRouting, Label)]
//! enum PingPong { Ping, Pong }
//! ```
use proc_macro::TokenStream;
use quote::quote;
use syn::{Data, DeriveInput, Fields, parse_macro_input};

/// Derives `Event` marker trait for the type.
#[proc_macro_derive(Event)]
pub fn derive_event(input: TokenStream) -> proc_macro::TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let ident = input.ident;
    let generics = input.generics;

    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

    let expanded = quote! {
        impl #impl_generics maiko::Event for #ident #ty_generics #where_clause {}
    };
    TokenStream::from(expanded)
}

/// Derives `Label` trait for enums, returning variant names.
///
/// For each variant, `label()` returns the variant name as a static string.
/// This is useful for logging, monitoring, and diagram generation.
///
/// # Example
///
/// ```rust,ignore
/// use maiko::Label;
///
/// #[derive(Label)]
/// enum MyTopic {
///     SensorData,
///     Alerts,
/// }
///
/// assert_eq!(MyTopic::SensorData.label(), "SensorData");
/// assert_eq!(MyTopic::Alerts.label(), "Alerts");
/// ```
///
/// # Panics
///
/// Compilation fails if used on a struct (only enums are supported).
#[proc_macro_derive(Label)]
pub fn derive_label(input: TokenStream) -> proc_macro::TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let ident = input.ident;
    let generics = input.generics;

    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

    let label_impl = match &input.data {
        Data::Enum(data_enum) => {
            let match_arms = data_enum.variants.iter().map(|variant| {
                let variant_ident = &variant.ident;
                let variant_name = variant_ident.to_string();

                // Handle different field types (unit, tuple, struct)
                let pattern = match &variant.fields {
                    Fields::Unit => quote! { Self::#variant_ident },
                    Fields::Unnamed(_) => quote! { Self::#variant_ident(..) },
                    Fields::Named(_) => quote! { Self::#variant_ident { .. } },
                };

                quote! {
                    #pattern => ::std::borrow::Cow::Borrowed(#variant_name)
                }
            });

            quote! {
                fn label(&self) -> ::std::borrow::Cow<'static, str> {
                    match self {
                        #(#match_arms),*
                    }
                }
            }
        }
        _ => {
            return syn::Error::new_spanned(ident, "Label can only be derived for enums")
                .to_compile_error()
                .into();
        }
    };

    let expanded = quote! {
        impl #impl_generics maiko::Label for #ident #ty_generics #where_clause {
            #label_impl
        }
    };
    TokenStream::from(expanded)
}

/// Derives `Topic for Self` enabling event-as-topic routing.
///
/// When an event type is used as its own topic, each variant becomes a distinct
/// routing category. This is common in systems like Kafka where topic names
/// match event types.
///
/// # Requirements
///
/// The type must also derive or implement:
/// - `Clone` (for `from_event` to clone the event)
/// - `Hash`, `PartialEq`, `Eq` (required by `Topic` trait)
/// - `Event` (to be used in the actor system)
///
/// # Example
///
/// ```rust,ignore
/// use maiko::{Event, SelfRouting};
///
/// #[derive(Clone, Debug, Hash, PartialEq, Eq, Event, SelfRouting)]
/// enum PingPongEvent {
///     Ping,
///     Pong,
/// }
///
/// // Now you can use PingPongEvent as both event and topic:
/// // Supervisor::<PingPongEvent>::default()
/// ```
#[proc_macro_derive(SelfRouting)]
pub fn derive_self_routing(input: TokenStream) -> proc_macro::TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let ident = input.ident;
    let generics = input.generics;

    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

    let expanded = quote! {
        impl #impl_generics maiko::Topic for #ident #ty_generics #where_clause {
            type Event = Self;
            fn from_event(event: &Self) -> Self {
                event.clone()
            }
        }
    };
    TokenStream::from(expanded)
}
