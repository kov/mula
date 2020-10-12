#![feature(proc_macro_diagnostic)]
use proc_macro::TokenStream;
use quote::quote;
use syn::parse_macro_input;
use syn::spanned::Spanned;
use syn;

/// Makes the function run only once at a time for a given input. Any calls made
/// with the same argument after the first will block and wait for it to finish
/// and get the result once it is available.
///
/// Functions can only be annotated with this macro if they take exactly one
/// argument and return exactly one value. This limitation may be lifted in the
/// future.
///
/// Both the input and output types must implement Clone and Send, and the
/// function itself needs to be Sync, since it will be executed by a separate
/// thread, potentially multiple threads if several different inputs are given.
///
///
/// # Example
///
/// The following example will only run the computation closure twice,
/// once for each of the distinct inputs. The two function calls for "burro"
/// will both be serviced by the same computation.
///
/// ```rust
/// use mula::mula;
///
/// #[mula]
/// fn delayed_uppercase(input: &'static str) -> String {
///     std::thread::sleep(std::time::Duration::from_secs(2));
///     input.to_uppercase()
/// }
///
/// let thread1 = std::thread::spawn(move || {
///     let upper = delayed_uppercase("mula");
///     assert_eq!(upper, "MULA".to_string());
/// });
///
/// let thread2 = std::thread::spawn(move || {
///     let upper = delayed_uppercase("burro");
///     assert_eq!(upper, "BURRO".to_string());
/// });
///
/// let thread3 = std::thread::spawn(move || {
///     let upper = delayed_uppercase("burro");
///     assert_eq!(upper, "BURRO".to_string());
/// });
///
/// thread1.join();
/// thread2.join();
/// thread3.join();
/// ```
#[proc_macro_attribute]
pub fn mula(_args: TokenStream, input: TokenStream) -> TokenStream {
    let mut mula_fn = parse_macro_input!(input as syn::ItemFn);

    // Rename the original function and create a wrapper with its name.
    let original_ident = mula_fn.sig.ident.clone();
    let wrapped_ident = syn::Ident::new(
        format!("{}_mula_fn", original_ident).as_str(),
        original_ident.span()
    );
    mula_fn.sig.ident = wrapped_ident.clone();

    let mula_ident = syn::Ident::new(
        format!("static_mula_for_{}", original_ident).to_uppercase().as_str(),
        original_ident.span()
    );

    let input_type = if let syn::FnArg::Typed(pat) = mula_fn.sig.inputs.first().unwrap() {
        &*pat.ty
    } else {
        return syn::Error::new(mula_fn.sig.span(), "mula functions must accept a single argument").to_compile_error().into();
    };

    let return_type = if let syn::ReturnType::Type(_, ty) = &mula_fn.sig.output {
        &**ty
    } else {
        return syn::Error::new(mula_fn.sig.span(), "mula functions must return a single value").to_compile_error().into();
    };

    let result = quote! {
        #mula_fn

        static #mula_ident: ::mula::once_cell::sync::OnceCell<std::sync::Arc<mula::Mula<#input_type, #return_type,  &'static (dyn Fn(#input_type) -> #return_type + Sync)>>> = ::mula::once_cell::sync::OnceCell::new();
        fn #original_ident(input: #input_type) -> #return_type {
            let m = #mula_ident.get_or_init(|| {
                ::mula::Mula::new(&#wrapped_ident)
            });
            let result = ::mula::Mula::subscribe_to(m.clone(), input.clone());
            result
        }
    };
    result.into()
}
