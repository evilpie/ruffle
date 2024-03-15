//! `Namespace` impl

use crate::avm2::activation::Activation;
use crate::avm2::object::{Object, TObject};
use crate::avm2::parameters::ParametersExt;
use crate::avm2::value::Value;
use crate::avm2::Error;
use crate::avm2::Namespace;
use crate::avm2_stub_constructor;

pub use crate::avm2::object::namespace_allocator;

pub fn call_handler<'gc>(
    activation: &mut Activation<'_, 'gc>,
    _this: Object<'gc>,
    args: &[Value<'gc>],
) -> Result<Value<'gc>, Error<'gc>> {
    panic!("call handler");
    Ok(Value::Undefined)
}

/// Implements `Namespace`'s instance initializer.
pub fn init<'gc>(
    activation: &mut Activation<'_, 'gc>,
    this: Object<'gc>,
    args: &[Value<'gc>],
) -> Result<Value<'gc>, Error<'gc>> {
    println!("init {args:?}");

    if let Some(this) = this.as_namespace_object() {
        let uri_value = match args.get_u32(activation, 0)? {
            // new Namespace ( )
            0 => None,
            // new Namespace ( uriValue )
            1 => Some(args[1]),
            // new Namespace ( prefixValue, uriValue )
            2.. => {
                avm2_stub_constructor!(activation, "Namespace", "Namespace prefix not supported");
                Some(args[2])
            }
        };

        let api_version = activation.avm2().root_api_version;

        let namespace = match uri_value {
            Some(Value::Object(Object::QNameObject(qname))) => qname
                .uri()
                .map(|uri| Namespace::package(uri, api_version, &mut activation.borrow_gc()))
                .unwrap_or_else(|| Namespace::any(activation.context.gc_context)),
            Some(val) => Namespace::package(
                val.coerce_to_string(activation)?,
                api_version,
                &mut activation.borrow_gc(),
            ),
            None => activation.avm2().public_namespace_base_version,
        };

        this.init_namespace(activation.context.gc_context, namespace);
    } else {
        panic!("what is going on");
    }
    Ok(Value::Undefined)
}


/// Implements `Namespace.uri`'s getter
pub fn get_uri<'gc>(
    _activation: &mut Activation<'_, 'gc>,
    this: Object<'gc>,
    _args: &[Value<'gc>],
) -> Result<Value<'gc>, Error<'gc>> {
    if let Some(o) = this.as_namespace_object() {
        return Ok(o.namespace().as_uri().into());
    }

    Ok(Value::Undefined)
}
