// Copyright 2020 TiKV Project Authors. Licensed under Apache-2.0.

mod reporter;

pub use reporter::*;

macro_rules! request_to_trace {
    ($($name: ident,)*) => {
        macro_rules! __may_enable_tracing {
            $(($name, $enable: expr) => { {
                if $enable {
                    let (root, collector) = Scope::root(stringify!($name));
                    (root, Some(collector))
                } else {
                    (Scope::empty(), None)
                }
            } };)*
            ($_: ident, $__: expr) => {{
                (Scope::empty(), None)
            }}
        }

        macro_rules! __may_set_trace_detail {
            $(($name, $resp_ty: ty) => {
                move |
                    resp: &mut $resp_ty,
                    trace_context: kvproto::kvrpcpb::TraceContext,
                    spans: Vec<tikv_util::trace::Span>,
                | {
                    let trace_detail = resp.mut_trace_detail();
                    let span_sets = trace_detail.mut_span_sets();
                    let span_set = span_sets.push_default();
                    span_set.set_node_type(kvproto::kvrpcpb::TraceDetailNodeType::TiKv);
                    span_set.set_root_parent_span_id(trace_context.get_root_parent_span_id());
                    span_set.set_trace_id(trace_context.get_trace_id());
                    span_set.set_root_parent_span_id(trace_context.get_root_parent_span_id());

                    let pb_spans = span_set.mut_spans();
                    for span in spans {
                        let pb_span = pb_spans.push_default();
                        pb_span.set_id(span.id);
                        pb_span.set_parent_id(span.parent_id);
                        pb_span.set_begin_unix_time_ns(span.begin_unix_time_ns);
                        pb_span.set_duration_ns(span.duration_ns);
                        pb_span.set_event(span.event.to_owned());
                        let pb_properties = pb_span.mut_properties();
                        for (k, v) in span.properties {
                            let pb_property = pb_properties.push_default();
                            pb_property.set_key(k.to_owned());
                            pb_property.set_value(v);
                        }
                    }
                }
            };)*
            ($_: ident, $__: ty) => { |_, _, _| {} }
        }
    }
}

// Register what kinds of request to trace.
request_to_trace!(
    coprocessor,
    kv_get,
    kv_scan,
    kv_prewrite,
    kv_pessimistic_lock,
    kv_commit,
    kv_batch_get,
);

macro_rules! trace_and_report {
    ($req_name: ident, $reporter: expr, $req: expr, $resp_ty: ty) => {{
        let req_trace_context = $req.mut_context().take_trace_context();
        let should_return = req_trace_context.get_is_enabled();

        let (scope, collector): (Scope, Option<Collector>) = __may_enable_tracing!(
            $req_name,
            !$reporter.subscribers_is_empty() || should_return
        );
        let reporter = $reporter.clone();

        let guard = start_scope(&scope);
        (guard, move |resp: &mut $resp_ty| {
            drop(scope);
            if let Some(spans) = reporter.collect(&req_trace_context, collector) {
                if should_return {
                    __may_set_trace_detail!($req_name, $resp_ty)(resp, req_trace_context, spans);
                }
            }
        })
    }};
}
