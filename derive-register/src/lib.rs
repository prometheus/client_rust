use quote::quote;

#[proc_macro_derive(Register)]
pub fn derive_register(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    quote! { ::std::compile_error!("todo") }.into()
}
