#![cfg_attr(not(feature = "verify_message"), allow(dead_code))]

use runtime::{Class, Sel};
use {Encode, EncodeArguments};
use super::MessageError;

pub fn verify_message_signature<A, R>(cls: &Class, sel: Sel)
        -> Result<(), MessageError>
        where A: EncodeArguments, R: Encode {
    let method = match cls.instance_method(sel) {
        Some(method) => method,
        None => return Err(MessageError(
            format!("Method {:?} not found on class {:?}",
                sel, cls)
        )),
    };

    let ret = R::encode();
    let expected_ret = method.return_type();
    if ret != expected_ret {
        return Err(MessageError(
            format!("Return type code {:?} does not match expected {:?} for method {:?}",
                ret, expected_ret, method.name())
        ));
    }

    let args = A::encodings();
    let args = args.as_ref();
    let count = args.len();
    let expected_count = method.arguments_count();
    if count != expected_count {
        return Err(MessageError(
            format!("Method {:?} accepts {} arguments, but {} were given",
                method.name(), expected_count, count)
        ));
    }

    for (i, arg) in args.iter().enumerate() {
        let expected = method.argument_type(i).unwrap();
        if *arg != expected {
            return Err(MessageError(
                format!("Method {:?} expected argument at index {} with type code {:?} but was given {:?}",
                    method.name(), i, expected, arg)
            ));
        }
    }

    Ok(())
}
