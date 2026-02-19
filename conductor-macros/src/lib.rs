// Copyright {{.Year}} Conductor OSS
// Licensed under the Apache License, Version 2.0. See LICENSE in the project root for license information.

use darling::{ast::NestedMeta, FromMeta};
use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::{format_ident, quote};
use syn::{parse_macro_input, FnArg, ItemFn, Pat, ReturnType};

/// Configuration options for the `#[worker]` attribute macro
#[derive(Debug, FromMeta, Default)]
struct WorkerArgs {
    /// Task definition name (defaults to function name)
    #[darling(default)]
    name: Option<String>,

    /// Poll interval in milliseconds (default: 100)
    #[darling(default)]
    poll_interval: Option<u64>,

    /// Maximum concurrent task executions (default: 1)
    #[darling(default)]
    thread_count: Option<usize>,

    /// Task routing domain (optional)
    #[darling(default)]
    domain: Option<String>,

    /// Worker identity (optional, defaults to hostname-pid)
    #[darling(default)]
    identity: Option<String>,
}

/// Marks an async function as a Conductor worker task.
///
/// This macro transforms a regular async function into a Conductor worker that can be
/// registered with a `TaskHandler`. The function's parameters are automatically extracted
/// from the task's input data.
///
/// # Attributes
///
/// - `name` - Task definition name (defaults to function name)
/// - `poll_interval` - Poll interval in milliseconds (default: 100)
/// - `thread_count` - Maximum concurrent executions (default: 1)
/// - `domain` - Task routing domain (optional)
/// - `identity` - Worker identity (optional)
///
/// # Function Signatures
///
/// ## Simple parameters extracted from task input
/// ```rust,ignore
/// #[worker(name = "greet")]
/// async fn greet(name: String) -> String {
///     format!("Hello, {}!", name)
/// }
/// ```
///
/// ## With Task parameter for full access
/// ```rust,ignore
/// #[worker(name = "process")]
/// async fn process(task: Task) -> WorkerOutput {
///     let data = task.get_input_string("data").unwrap();
///     WorkerOutput::completed_with_result(data)
/// }
/// ```
///
/// ## With TaskContext for metadata access
/// ```rust,ignore
/// #[worker(name = "long_running")]
/// async fn long_running(ctx: TaskContext, batch_size: i32) -> WorkerOutput {
///     let offset = ctx.poll_count() * batch_size;
///     // Process batch at offset...
///     
///     if ctx.is_first_poll() {
///         println!("Starting task {}", ctx.task_id());
///     }
///     
///     WorkerOutput::in_progress(30)
/// }
/// ```
///
/// ## Combined Task and TaskContext
/// ```rust,ignore
/// #[worker(name = "advanced")]
/// async fn advanced(task: Task, ctx: TaskContext) -> WorkerOutput {
///     // Full task access and convenient context methods
///     let input: String = task.get_input("data").unwrap_or_default();
///     println!("Processing task {} (poll #{})", ctx.task_id(), ctx.poll_count());
///     WorkerOutput::completed_with_result(input)
/// }
/// ```
///
/// # Return Types
///
/// - `String` or any serializable type - Wrapped in `WorkerOutput::completed_with_result()`
/// - `WorkerOutput` - Used directly  
/// - `Result<T, E>` - Success wrapped in completed, error converted to failed
///
/// # Generated Code
///
/// The macro generates a function `{fn_name}_worker()` that returns an `FnWorker`:
///
/// ```rust,ignore
/// #[worker(name = "greet", thread_count = 5)]
/// async fn greet(name: String) -> String {
///     format!("Hello, {}!", name)
/// }
///
/// // Generates:
/// fn greet_worker() -> FnWorker {
///     FnWorker::new("greet", |task| async move {
///         let name: String = task.get_input("name").unwrap_or_default();
///         let result = format!("Hello, {}!", name);
///         Ok(WorkerOutput::completed_with_result(result))
///     })
///     .with_thread_count(5)
/// }
/// ```
///
/// # Usage
///
/// ```rust,ignore
/// use conductor_macros::worker;
/// use conductor::{TaskHandler, Configuration};
///
/// #[worker(name = "process_order", thread_count = 10, domain = "orders")]
/// async fn process_order(order_id: String, amount: f64) -> serde_json::Value {
///     serde_json::json!({
///         "order_id": order_id,
///         "amount": amount,
///         "status": "processed"
///     })
/// }
///
/// #[tokio::main]
/// async fn main() {
///     let config = Configuration::default();
///     let mut handler = TaskHandler::new(config).unwrap();
///     
///     // Use the generated worker function
///     handler.add_worker(process_order_worker());
///     
///     handler.start().await.unwrap();
/// }
/// ```
#[proc_macro_attribute]
pub fn worker(args: TokenStream, input: TokenStream) -> TokenStream {
    let attr_args = match NestedMeta::parse_meta_list(args.into()) {
        Ok(v) => v,
        Err(e) => {
            return TokenStream::from(darling::Error::from(e).write_errors());
        }
    };

    let args = match WorkerArgs::from_list(&attr_args) {
        Ok(v) => v,
        Err(e) => {
            return TokenStream::from(e.write_errors());
        }
    };

    let input_fn = parse_macro_input!(input as ItemFn);

    match generate_worker(args, input_fn) {
        Ok(tokens) => tokens.into(),
        Err(e) => e.to_compile_error().into(),
    }
}

fn generate_worker(args: WorkerArgs, input_fn: ItemFn) -> syn::Result<TokenStream2> {
    let fn_name = &input_fn.sig.ident;
    let fn_vis = &input_fn.vis;
    let fn_block = &input_fn.block;
    let fn_inputs = &input_fn.sig.inputs;
    let fn_output = &input_fn.sig.output;

    // Ensure function is async
    if input_fn.sig.asyncness.is_none() {
        return Err(syn::Error::new_spanned(
            &input_fn.sig,
            "worker function must be async",
        ));
    }

    // Task name defaults to function name
    let task_name = args.name.unwrap_or_else(|| fn_name.to_string());

    // Worker function name (appends _worker)
    let worker_fn_name = format_ident!("{}_worker", fn_name);

    // Configuration values
    let poll_interval = args.poll_interval.unwrap_or(100);
    let thread_count = args.thread_count.unwrap_or(1);

    // Analyze parameters
    let mut param_extractions = Vec::new();
    let mut fn_args = Vec::new();
    let mut has_task_param = false;
    let mut has_context_param = false;

    for arg in fn_inputs {
        match arg {
            FnArg::Receiver(_) => {
                return Err(syn::Error::new_spanned(
                    arg,
                    "worker functions cannot have self parameter",
                ));
            }
            FnArg::Typed(pat_type) => {
                let name = match &*pat_type.pat {
                    Pat::Ident(ident) => ident.ident.clone(),
                    _ => {
                        return Err(syn::Error::new_spanned(
                            &pat_type.pat,
                            "expected simple identifier pattern",
                        ));
                    }
                };

                let ty = &pat_type.ty;
                let ty_str = quote!(#ty).to_string().replace(' ', "");

                // Check if this is the TaskContext parameter
                if ty_str.contains("TaskContext") {
                    has_context_param = true;
                    fn_args.push(quote! { __ctx });
                // Check if this is the Task parameter
                } else if ty_str.contains("Task") {
                    has_task_param = true;
                    fn_args.push(quote! { task.clone() });
                } else {
                    // Regular parameter - extract from task input
                    let name_str = name.to_string();
                    param_extractions.push(quote! {
                        let #name: #ty = task.get_input(#name_str).unwrap_or_default();
                    });
                    fn_args.push(quote! { #name });
                }
            }
        }
    }

    // Generate TaskContext extraction if needed
    let context_extraction = if has_context_param {
        quote! {
            let __ctx = task.context();
        }
    } else {
        quote! {}
    };

    // Generate return handling based on return type
    let return_handling = match fn_output {
        ReturnType::Default => {
            quote! {
                Ok(::conductor::worker::WorkerOutput::completed_with_result(()))
            }
        }
        ReturnType::Type(_, ty) => {
            let ty_str = quote!(#ty).to_string().replace(' ', "");

            if ty_str.contains("WorkerOutput") {
                quote! { Ok(result) }
            } else if ty_str.starts_with("Result<") || ty_str.contains("::Result<") {
                quote! {
                    match result {
                        Ok(value) => Ok(::conductor::worker::WorkerOutput::completed_with_result(value)),
                        Err(e) => Ok(::conductor::worker::WorkerOutput::failed(format!("{}", e))),
                    }
                }
            } else {
                quote! {
                    Ok(::conductor::worker::WorkerOutput::completed_with_result(result))
                }
            }
        }
    };

    // Generate domain configuration
    let domain_config = if let Some(domain) = &args.domain {
        quote! { .with_domain(#domain) }
    } else {
        quote! {}
    };

    // Generate identity configuration
    let identity_config = if let Some(identity) = &args.identity {
        quote! { .with_identity(#identity) }
    } else {
        quote! {}
    };

    // Build the async block body
    let async_body = if has_task_param || has_context_param {
        quote! {
            #context_extraction
            #(#param_extractions)*
            let result = (|#fn_inputs| async move #fn_block)(#(#fn_args),*).await;
            #return_handling
        }
    } else if !fn_args.is_empty() {
        quote! {
            #(#param_extractions)*
            let result = (|#fn_inputs| async move #fn_block)(#(#fn_args),*).await;
            #return_handling
        }
    } else {
        quote! {
            #(#param_extractions)*
            let result = (|| async move #fn_block)().await;
            #return_handling
        }
    };

    // Generate the output
    let output = quote! {
        /// Creates a worker for the `#task_name` task.
        ///
        /// Generated by the `#[worker]` macro from the `#fn_name` function.
        #fn_vis fn #worker_fn_name() -> ::conductor::worker::FnWorker {
            ::conductor::worker::FnWorker::new(#task_name, |task: ::conductor::models::Task| async move {
                #async_body
            })
            .with_poll_interval_millis(#poll_interval)
            .with_thread_count(#thread_count)
            #domain_config
            #identity_config
        }
    };

    Ok(output)
}

/// Alias for `#[worker]` - marks a function as a Conductor worker task.
///
/// This is provided for familiarity with Python SDK's `@worker_task` decorator.
#[proc_macro_attribute]
pub fn worker_task(args: TokenStream, input: TokenStream) -> TokenStream {
    worker(args, input)
}
