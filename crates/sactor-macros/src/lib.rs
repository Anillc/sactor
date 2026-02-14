use manyhow::manyhow;
use proc_macro2::TokenStream;
use quote::quote;
use syn::{
    Error, FnArg, GenericParam, Ident, ImplItem, ImplItemFn, ItemImpl, Pat, PatIdent, Result,
    ReturnType, Type, Visibility, parse2, spanned::Spanned,
};

#[manyhow]
#[proc_macro_attribute]
pub fn sactor(attr: TokenStream, item: TokenStream) -> Result<TokenStream> {
    let handle_vis: Visibility = if attr.is_empty() {
        Visibility::Inherited
    } else {
        parse2(attr)?
    };
    let mut input: ItemImpl = parse2(item)?;
    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();

    let self_ident = {
        let Type::Path(path) = input.self_ty.as_ref() else {
            return Err(Error::new_spanned(&input.self_ty, "expected a path"));
        };
        path.path.segments.last().unwrap().ident.clone()
    };
    let handle_ident = Ident::new(&format!("{}Handle", self_ident), self_ident.span());
    let events_ident = Ident::new(&format!("{}Events", self_ident), self_ident.span());

    let type_params: Vec<_> = input
        .generics
        .params
        .iter()
        .filter_map(|p| {
            if let GenericParam::Type(tp) = p {
                Some(&tp.ident)
            } else {
                None
            }
        })
        .collect();

    let mut event_variants = Vec::new();
    let mut handle_items = Vec::new();
    let mut run_arms = Vec::new();
    for item in &mut input.items {
        let ImplItem::Fn(ImplItemFn {
            attrs, vis, sig, ..
        }) = item
        else {
            continue;
        };
        if sig.inputs.is_empty() {
            continue;
        }
        match sig.inputs.first().unwrap() {
            FnArg::Typed(_) => continue,
            FnArg::Receiver(receiver) if receiver.reference.is_none() => continue,
            _ => {}
        }

        // reject method-level generics
        if !sig.generics.params.is_empty() {
            return Err(Error::new_spanned(
                &sig.generics,
                "should not have method-level generics",
            ));
        }

        // need a reply?
        let mut reply = false;
        attrs.retain(|attr| {
            if attr.meta.path().is_ident("reply") {
                reply = true;
                return false;
            }
            true
        });

        // output type
        let output = match &sig.output {
            ReturnType::Default => quote! { () },
            ReturnType::Type(_, ty) => {
                reply = true;
                quote! { #ty }
            }
        };
        let mut handle_sig = sig.clone();
        handle_sig.asyncness = Some(parse2(quote! { async })?);
        handle_sig.output = parse2(quote! { -> sactor::error::SactorResult<#output> })?;

        // input args
        let mut arg_types = Vec::new();
        let mut arg_names = Vec::new();
        for (i, arg) in &mut handle_sig.inputs.iter_mut().enumerate() {
            let arg = match arg {
                FnArg::Typed(arg) => arg,
                FnArg::Receiver(arg) => {
                    arg.mutability = None;
                    let Type::Reference(reference) = arg.ty.as_mut() else {
                        return Err(Error::new_spanned(&arg.ty, "expected a reference"));
                    };
                    reference.mutability = None;
                    continue;
                }
            };
            arg_types.push(arg.ty.clone());
            let arg_name = format!("arg{}", i);
            arg_names.push(Ident::new(&arg_name, arg.pat.span()));
            *arg.pat = Pat::Ident(PatIdent {
                attrs: Vec::new(),
                by_ref: None,
                mutability: None,
                ident: Ident::new(&arg_name, arg.pat.span()),
                subpat: None,
            });
        }

        // event type and args
        let event_name = &sig.ident;
        let arg_typle_type = quote! { (#(#arg_types),*) };
        let arg_tuple = quote! { (#(#arg_names),*) };

        let f = if reply {
            quote! {
                #vis #handle_sig {
                    let (tx, rx) = tokio::sync::oneshot::channel();
                    self.0.send(#events_ident::#event_name(#arg_tuple, tx))
                        .map_err(|_| sactor::error::SactorError::ActorStopped)?;
                    Ok(rx.await.map_err(|_| sactor::error::SactorError::ActorStopped)?)
                }
            }
        } else {
            quote! {
                #vis #handle_sig {
                    self.0.send(#events_ident::#event_name(#arg_tuple))
                        .map_err(|_| sactor::error::SactorError::ActorStopped)?;
                    Ok(())
                }
            }
        };

        handle_items.push(f);
        if reply {
            event_variants.push(
                quote! { #event_name(#arg_typle_type, tokio::sync::oneshot::Sender<#output>) },
            );
            if sig.asyncness.is_some() {
                run_arms.push(quote! {
                    Some(#events_ident::#event_name(#arg_tuple, tx)) => {
                        let fut = self.#event_name #arg_tuple.await;
                        let _ = tx.send(fut);
                    }
                });
            } else {
                run_arms.push(quote! {
                    Some(#events_ident::#event_name(#arg_tuple, tx)) => {
                        let res = self.#event_name #arg_tuple;
                        let _ = tx.send(res);
                    }
                });
            }
        } else {
            event_variants.push(quote! { #event_name(#arg_typle_type) });
            if sig.asyncness.is_some() {
                run_arms.push(quote! {
                    Some(#events_ident::#event_name(#arg_tuple)) => {
                        self.#event_name #arg_tuple.await;
                    }
                });
            } else {
                run_arms.push(quote! {
                    Some(#events_ident::#event_name(#arg_tuple)) => {
                        self.#event_name #arg_tuple;
                    }
                });
            }
        }
    }

    input.items.push(parse2(quote! {
        fn run(mut self) -> (impl Future<Output = ()>, #handle_ident #ty_generics) {
            let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
            let handle = #handle_ident(tx);
            let future = async move {
                loop {
                    match rx.recv().await {
                        #(#run_arms),*
                        Some(#events_ident::stop) | None => break,
                        Some(#events_ident::phantom(_)) => unreachable!(),
                    }
                }
            };
            (future, handle)
        }
    })?);

    Ok(quote! {
        #input

        #[allow(non_camel_case_types)]
        enum #events_ident #impl_generics #where_clause {
            #(#event_variants),*,
            stop,
            phantom(std::marker::PhantomData<(#(#type_params),*)>),
        }

        #[derive(Clone)]
        #handle_vis struct #handle_ident #impl_generics #where_clause (tokio::sync::mpsc::UnboundedSender<#events_ident #ty_generics>);
        impl #impl_generics #handle_ident #ty_generics #where_clause {
            #(#handle_items)*

            #handle_vis fn is_running(&self) -> bool {
                !self.0.is_closed()
            }

            #handle_vis fn stop(&self) {
                let _ = self.0.send(#events_ident::stop);
            }
        }
    })
}
