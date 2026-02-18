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
    let mut sel = None; // select ident and asyncness
    let mut error_handler = None;
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

        let mut skip = false;
        let mut reply = None;
        let mut select = false;
        let mut error = false;
        attrs.retain(|attr| {
            let path = attr.meta.path();
            if path.is_ident("skip") {
                skip = true;
                return false;
            }
            if path.is_ident("reply") {
                reply = Some(true);
                return false;
            }
            if path.is_ident("no_reply") {
                reply = Some(false);
                return false;
            }
            if path.is_ident("select") {
                select = true;
                return false;
            }
            if path.is_ident("handle_error") {
                error = true;
                return false;
            }
            true
        });
        if select {
            if sel.is_some() {
                return Err(Error::new_spanned(
                    &sig.ident,
                    "multiple select methods are not allowed",
                ));
            }
            sel = Some((sig.ident.clone(), sig.asyncness.is_some()));
            continue;
        }
        if error {
            if error_handler.is_some() {
                return Err(Error::new_spanned(
                    &sig.ident,
                    "multiple error handler methods are not allowed",
                ));
            }
            error_handler = Some((sig.ident.clone(), sig.asyncness.is_some()));
            continue;
        }
        if skip {
            continue;
        }

        // reject method-level generics
        if !sig.generics.params.is_empty() {
            return Err(Error::new_spanned(
                &sig.generics,
                "should not have method-level generics",
            ));
        }

        // output type
        let mut handle_error = false;
        let output = match (&sig.output, &reply) {
            (ReturnType::Default, _) | (_, Some(false)) => quote! { () },
            (ReturnType::Type(_, ty), _) => {
                if reply.is_none() {
                    reply = Some(true);
                }
                if let Type::Path(path) = ty.as_ref() {
                    let Some(last) = path.path.segments.last() else {
                        return Err(Error::new_spanned(
                            &path.path,
                            "expected a path with segments",
                        ));
                    };
                    if last.ident == "SactorResult" {
                        handle_error = true;
                    }
                }
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

        let f = if reply.unwrap_or(false) {
            quote! {
                #vis #handle_sig {
                    let (tx, rx) = futures::channel::oneshot::channel();
                    self.0.unbounded_send(#events_ident::#event_name(#arg_tuple, tx))
                        .map_err(|_| sactor::error::SactorError::ActorStopped)?;
                    #[allow(clippy::needless_question_mark)]
                    Ok(rx.await.map_err(|_| sactor::error::SactorError::ActorStopped)?)
                }
            }
        } else {
            quote! {
                #vis #handle_sig {
                    self.0.unbounded_send(#events_ident::#event_name(#arg_tuple))
                        .map_err(|_| sactor::error::SactorError::ActorStopped)?;
                    Ok(())
                }
            }
        };

        handle_items.push(f);

        let aw = match sig.asyncness {
            None => quote! {},
            Some(_) => quote! { .await },
        };
        let handle_error = match handle_error {
            false => quote! {},
            true => quote! {
                if let Err(e) = &mut result {
                    actor.__sactor_handle_error(e).await;
                }
            },
        };
        if reply.unwrap_or(false) {
            event_variants.push(
                quote! { #event_name(#arg_typle_type, futures::channel::oneshot::Sender<#output>) },
            );
            run_arms.push(quote! {
                Ok(#events_ident::#event_name(#arg_tuple, tx)) => {
                    let mut result = actor.#event_name #arg_tuple #aw;
                    #handle_error;
                    let _ = tx.send(result);
                }
            });
        } else {
            event_variants.push(quote! { #event_name(#arg_typle_type) });
            run_arms.push(quote! {
                Ok(#events_ident::#event_name(#arg_tuple)) => {
                    let mut result = actor.#event_name #arg_tuple #aw;
                    #handle_error;
                }
            });
        }
    }

    let select = match sel {
        None => quote! {
            let sel = std::future::pending::<(#events_ident #ty_generics, usize, Vec<Selection>)>();
        },
        Some((sel, false)) => quote! {
            let futures: Vec<Selection> = actor.#sel();
            let sel = futures::future::select_all(futures);
        },
        Some((sel, true)) => quote! {
            let futures: Vec<Selection> = actor.#sel().await;
            let sel = futures::future::select_all(futures);
        },
    };

    input.items.push(parse2(quote! {
        fn run<F>(init: F) -> (impl Future<Output = ()>, #handle_ident #ty_generics)
        where
            F: FnOnce(#handle_ident #ty_generics) -> Self,
        {
            use futures::FutureExt as _;
            let (tx, mut rx) = futures::channel::mpsc::unbounded();
            let handle = #handle_ident(tx);
            let mut actor = init(handle.clone());
            let handle2 = handle.clone();
            let future = async move {
                loop {
                    #select
                    futures::select_biased! {
                        event = rx.recv() => {
                            match event {
                                #(#run_arms),*
                                Ok(#events_ident::__sactor_stop) | Err(_) => break,
                                Ok(#events_ident::__sactor_phantom(_)) => unreachable!(),
                            }
                        }
                        event = async { sel.await.0 }.fuse() => {
                            handle2.0.unbounded_send(event).unwrap();
                        }
                    }
                }
            };
            (future, handle)
        }
    })?);

    let call_error_handler = match error_handler {
        None => quote! {},
        Some((error_handler, false)) => quote! {
            self.#error_handler(error);
        },
        Some((error_handler, true)) => quote! {
            self.#error_handler(error).await;
        },
    };
    input.items.push(parse2(quote! {
        async fn __sactor_handle_error(&mut self, error: &mut sactor::error::SactorError) {
            #call_error_handler
        }
    })?);

    Ok(quote! {
        type Selection<'a> = std::pin::Pin<Box<dyn Future<Output = #events_ident #ty_generics> + Send + 'a>>;

        #[allow(unused_macros)]
        macro_rules! selection {
            ($expression:expr, $variant:ident) => {
                Box::pin(async { $expression; #events_ident::$variant(()) }) as Selection
            };
            ($expression:expr, $variant:ident, $name:pat => $($arg:tt)*) => {
                Box::pin(async { let $name = $expression; #events_ident::$variant($($arg)*) }) as Selection
            };
        }

        #input

        #[allow(non_camel_case_types)]
        enum #events_ident #impl_generics #where_clause {
            __sactor_stop,
            __sactor_phantom(std::marker::PhantomData<(#(#type_params),*)>),
            #(#event_variants),*
        }

        #[derive(Clone)]
        #handle_vis struct #handle_ident #impl_generics #where_clause (futures::channel::mpsc::UnboundedSender<#events_ident #ty_generics>);
        impl #impl_generics #handle_ident #ty_generics #where_clause {
            #(#handle_items)*

            #handle_vis fn is_running(&self) -> bool {
                !self.0.is_closed()
            }

            #handle_vis fn stop(&self) {
                let _ = self.0.unbounded_send(#events_ident::__sactor_stop);
            }
        }
    })
}
