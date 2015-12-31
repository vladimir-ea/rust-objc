use std::any::Any;
use std::error::Error;
use std::fmt;
use std::mem;

use runtime::{Class, Imp, Object, Sel, Super};

mod verify;

#[cfg(target_arch = "x86")]
#[path = "x86.rs"]
mod platform;
#[cfg(target_arch = "x86_64")]
#[path = "x86_64.rs"]
mod platform;
#[cfg(target_arch = "arm")]
#[path = "arm.rs"]
mod platform;
#[cfg(all(target_arch = "aarch64", not(feature = "gnustep")))]
#[path = "arm64.rs"]
mod platform;

#[cfg(feature= "gnustep")]
mod gnustep;

#[cfg(not(feature = "gnustep"))]
use self::platform::{msg_send_fn, msg_send_super_fn};
#[cfg(feature = "gnustep")]
use self::gnustep::{msg_send_fn, msg_send_super_fn};

/// Types that may be sent Objective-C messages.
/// For example: objects, classes, and blocks.
pub unsafe trait Message { }

unsafe impl Message for Object { }

unsafe impl Message for Class { }

/// Types that may be used as the arguments of an Objective-C message.
pub trait MessageArguments: Sized {
    unsafe fn invoke<R>(imp: Imp, obj: *mut Object, sel: Sel, args: Self) -> R
            where R: Any;

    /// Sends a message to the given obj with the given selector and self as
    /// the arguments.
    ///
    /// The correct version of `objc_msgSend` will be chosen based on the
    /// return type. For more information, see Apple's documenation:
    /// https://developer.apple.com/library/mac/documentation/Cocoa/Reference/ObjCRuntimeRef/index.html#//apple_ref/doc/uid/TP40001418-CH1g-88778
    ///
    /// It is recommended to use the `msg_send!` macro rather than calling this
    /// method directly.
    unsafe fn send<T, R>(self, obj: *mut T, sel: Sel) -> R
            where T: Message, R: Any {
        send_unverified(obj, sel, self).unwrap()
    }

    /// Sends a message to the superclass of an instance of a class with self
    /// as the arguments.
    unsafe fn send_super<T, R>(self, obj: *mut T, superclass: &Class, sel: Sel) -> R
            where T: Message, R: Any {
        send_super_unverified(obj, superclass, sel, self).unwrap()
    }
}

macro_rules! message_args_impl {
    ($($a:ident : $t:ident),*) => (
        impl<$($t),*> MessageArguments for ($($t,)*) {
            unsafe fn invoke<R>(imp: Imp, obj: *mut Object, sel: Sel, ($($a,)*): Self) -> R
                    where R: Any {
                let imp: unsafe extern fn(*mut Object, Sel $(, $t)*) -> R =
                    mem::transmute(imp);
                imp(obj, sel $(, $a)*)
            }
        }
    );
}

message_args_impl!();
message_args_impl!(a: A);
message_args_impl!(a: A, b: B);
message_args_impl!(a: A, b: B, c: C);
message_args_impl!(a: A, b: B, c: C, d: D);
message_args_impl!(a: A, b: B, c: C, d: D, e: E);
message_args_impl!(a: A, b: B, c: C, d: D, e: E, f: F);
message_args_impl!(a: A, b: B, c: C, d: D, e: E, f: F, g: G);
message_args_impl!(a: A, b: B, c: C, d: D, e: E, f: F, g: G, h: H);
message_args_impl!(a: A, b: B, c: C, d: D, e: E, f: F, g: G, h: H, i: I);
message_args_impl!(a: A, b: B, c: C, d: D, e: E, f: F, g: G, h: H, i: I, j: J);
message_args_impl!(a: A, b: B, c: C, d: D, e: E, f: F, g: G, h: H, i: I, j: J, k: K);
message_args_impl!(a: A, b: B, c: C, d: D, e: E, f: F, g: G, h: H, i: I, j: J, k: K, l: L);

#[derive(Debug)]
pub struct MessageError(String);

impl fmt::Display for MessageError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fmt::Display::fmt(&self.0, f)
    }
}

impl Error for MessageError {
    fn description(&self) -> &str {
        &self.0
    }
}

#[cfg(feature = "exception")]
macro_rules! objc_try {
    ($b:block) => (
        $crate::exception::try(|| $b).map_err(|exception| match exception {
            Some(exception) => MessageError(format!("Uncaught exception {:?}", &*exception)),
            None => MessageError("Uncaught exception nil".to_owned()),
        })
    )
}

#[cfg(not(feature = "exception"))]
macro_rules! objc_try {
    ($b:block) => (Ok($b))
}

#[inline(always)]
unsafe fn send_unverified<T, A, R>(obj: *const T, sel: Sel, args: A)
        -> Result<R, MessageError>
        where T: Message, A: MessageArguments, R: Any {
    let (msg_send_fn, receiver) = msg_send_fn::<R>(obj as *mut T as *mut Object, sel);
    objc_try!({
        A::invoke(msg_send_fn, receiver, sel, args)
    })
}

#[doc(hidden)]
#[inline(always)]
#[cfg(not(feature = "verify_message"))]
pub unsafe fn send_message<T, A, R>(obj: *const T, sel: Sel, args: A)
        -> Result<R, MessageError>
        where T: Message, A: MessageArguments, R: Any {
    send_unverified(obj, sel, args)
}

#[doc(hidden)]
#[inline(always)]
#[cfg(feature = "verify_message")]
pub unsafe fn send_message<T, A, R>(obj: *const T, sel: Sel, args: A)
        -> Result<R, MessageError>
        where T: Message, A: MessageArguments + ::EncodeArguments,
        R: Any + ::Encode {
    use self::verify::verify_message_signature;

    let cls = if obj.is_null() {
        return Err(MessageError(format!("Messaging {:?} to nil", sel)));
    } else {
        (*(obj as *const Object)).class()
    };

    verify_message_signature::<A, R>(cls, sel).and_then(|_| {
        send_unverified(obj, sel, args)
    })
}

#[inline(always)]
unsafe fn send_super_unverified<T, A, R>(obj: *const T, superclass: &Class,
        sel: Sel, args: A) -> Result<R, MessageError>
        where T: Message, A: MessageArguments, R: Any {
    let sup = Super { receiver: obj as *mut T as *mut Object, superclass: superclass };
    let (msg_send_fn, receiver) = msg_send_super_fn::<R>(&sup, sel);
    objc_try!({
        A::invoke(msg_send_fn, receiver, sel, args)
    })
}

#[doc(hidden)]
#[inline(always)]
#[cfg(not(feature = "verify_message"))]
pub unsafe fn send_super_message<T, A, R>(obj: *const T, superclass: &Class,
        sel: Sel, args: A) -> Result<R, MessageError>
        where T: Message, A: MessageArguments, R: Any {
    send_super_unverified(obj, superclass, sel, args)
}

#[doc(hidden)]
#[inline(always)]
#[cfg(feature = "verify_message")]
pub unsafe fn send_super_message<T, A, R>(obj: *const T, superclass: &Class,
        sel: Sel, args: A) -> Result<R, MessageError>
        where T: Message, A: MessageArguments + ::EncodeArguments,
        R: Any + ::Encode {
    use self::verify::verify_message_signature;

    if obj.is_null() {
        return Err(MessageError(format!("Messaging {:?} to nil", sel)));
    }

    verify_message_signature::<A, R>(superclass, sel).and_then(|_| {
        send_super_unverified(obj, superclass, sel, args)
    })
}

#[cfg(test)]
mod tests {
    use runtime::Object;
    use test_utils;

    #[test]
    fn test_send_message() {
        let obj = test_utils::custom_object();
        let result: u32 = unsafe {
            let _: () = msg_send![obj, setFoo:4u32];
            msg_send![obj, foo]
        };
        assert!(result == 4);
    }

    #[test]
    fn test_send_message_stret() {
        let obj = test_utils::custom_object();
        let result: test_utils::CustomStruct = unsafe {
            msg_send![obj, customStruct]
        };
        let expected = test_utils::CustomStruct { a: 1, b:2, c: 3, d: 4 };
        assert!(result == expected);
    }

    #[test]
    fn test_send_message_nil() {
        let nil: *mut Object = ::std::ptr::null_mut();
        let result: usize = unsafe {
            msg_send![nil, hash]
        };
        assert!(result == 0);

        let result: *mut Object = unsafe {
            msg_send![nil, description]
        };
        assert!(result.is_null());

        let result: f64 = unsafe {
            msg_send![nil, doubleValue]
        };
        assert!(result == 0.0);
    }

    #[test]
    fn test_send_message_super() {
        let obj = test_utils::custom_subclass_object();
        let superclass = test_utils::custom_class();
        unsafe {
            let _: () = msg_send![obj, setFoo:4u32];
            let foo: u32 = msg_send![super(obj, superclass), foo];
            assert!(foo == 4);

            // The subclass is overriden to return foo + 2
            let foo: u32 = msg_send![obj, foo];
            assert!(foo == 6);
        }
    }
}
