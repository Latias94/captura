use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, ItemConst};

/// Attribute macro used on a `Route` constant to auto-register it
/// into the Hub registry.
///
/// Example:
///
/// ```ignore
/// use captura_hub_macros::register_hub_route;
/// use crate::hub::types::Route;
///
/// #[register_hub_route]
/// pub const ROUTE_FOO: Route = Route { /* ... */ };
/// ```
#[proc_macro_attribute]
pub fn register_hub_route(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item as ItemConst);
    let name = &input.ident;

    let expanded = quote! {
        #input

        #[doc(hidden)]
        #[allow(non_upper_case_globals)]
        inventory::submit! {
            crate::hub::types::RouteWrapper(#name)
        }
    };

    TokenStream::from(expanded)
}
