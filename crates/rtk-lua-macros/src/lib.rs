use proc_macro::TokenStream;

/// Dummy proc macro that does nothing and relays the item it sits on top of back out. This is used
/// to specify marker overrides that are used when dogfooding the lua api
#[proc_macro_derive(RtkMeta, attributes(rtk_meta))]
pub fn rtk_meta(_attrs: TokenStream) -> TokenStream {
    TokenStream::new()
}
