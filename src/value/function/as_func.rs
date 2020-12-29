use super::ArgsIter;
use crate::{Args, Ctx, Error, FromJs, Function, IntoJs, Method, Result, This, Value};

#[cfg(feature = "classes")]
use crate::{Class, ClassDef, Constructor};

/// The trait to wrap rust function to JS directly
pub trait AsFunction<'js, A, R> {
    /// Minimum number of arguments
    const LEN: u32;

    /// Calling function from JS side
    fn call(&self, ctx: Ctx<'js>, this: Value<'js>, args: ArgsIter<'js>) -> Result<Value<'js>>;

    /// Post-processing the function
    fn post<'js_>(_ctx: Ctx<'js_>, _func: &Function<'js_>) -> Result<()> {
        Ok(())
    }
}

/// The trait to wrap rust function to JS directly
pub trait AsFunctionMut<'js, A, R> {
    /// Minimum number of arguments
    const LEN: u32;

    /// Calling function from JS side
    fn call(&mut self, ctx: Ctx<'js>, this: Value<'js>, args: ArgsIter<'js>) -> Result<Value<'js>>;

    /// Post-processing the function
    fn post<'js_>(_ctx: Ctx<'js_>, _func: &Function<'js_>) -> Result<()> {
        Ok(())
    }
}

macro_rules! as_fn_impls {
    ($($($t:ident)*,)*) => {
        $(
            // for Method<Fn>
            as_fn_impls!(@fun [Method AsFunction &] $($t)*);
            // for Constructor<Fn>
            as_fn_impls!(@fun [Constructor AsFunction &] $($t)*);
            // for Fn
            as_fn_impls!(@fun [Fn AsFunction &] $($t)*);
            // for FnMut
            as_fn_impls!(@fun [FnMut AsFunctionMut &mut] $($t)*);
        )*
    };

    (@fun [$($f:tt)*] $($t:ident)*) => {
        // -varargs
        as_fn_impls!(@gen [$($f)*] $($t)*; :;);
        // +varargs
        as_fn_impls!(@gen [$($f)*] $($t)*; X: [Args<X>];);
    };

    (@gen [$($f:tt)*] $($t:ident)*; $($s:tt)*) => {
        // -ctx -this
        as_fn_impls!(@imp [$($f)*] $($t)*; :; $($s)*);
        // +ctx -this
        as_fn_impls!(@imp [$($f)*] $($t)*; : [Ctx<'js>]; $($s)*);
        // -ctx +this
        as_fn_impls!(@imp [$($f)*] $($t)*; T: [This<T>]; $($s)*);
        // +ctx +this
        as_fn_impls!(@imp [$($f)*] $($t)*; T: [Ctx<'js>], [This<T>]; $($s)*);
    };

    // $i - trait name (AsFunction or AsFunctionMut)
    // $s - self reference (& or &mut)
    // $t - argument type parameters
    // $tp - preceded type parameters
    // $ts - succeeded type parameters
    // $ap - preceded arg types
    // $as - succeeded arg types
    (@imp [Method $i:tt $($s:tt)*] $($t:ident)*; $($tp:ident)*: $([$($ap:tt)*]),*; $($ts:ident)*: $([$($as:tt)*]),*; ) => {
        impl<'js, F, S, $($tp,)* $($t,)* $($ts,)* R> $i<'js, (S, $($($ap)*,)* $($t,)* $($($as)*,)*), R> for Method<F>
        where
            F: Fn(S, $($($ap)*,)* $($t,)* $($($as)*,)*) -> R,
            S: FromJs<'js>,
            $($tp: FromJs<'js>,)*
            $($t: FromJs<'js>,)*
            $($ts: FromJs<'js>,)*
            R: IntoJs<'js>,
        {
            const LEN: u32 = 0 $(+ as_fn_impls!(@one $t))*;

            #[allow(unused_mut, unused)]
            fn call($($s)* self, ctx: Ctx<'js>, this: Value<'js>, mut args: ArgsIter<'js>) -> Result<Value<'js>> {
                self(
                    S::from_js(ctx, this.clone())?,
                    $(as_fn_impls!(@arg ctx this args $($ap)*),)*
                    $($t::from_js(ctx, args.next().ok_or_else(not_enough_args)?)?,)*
                    $(as_fn_impls!(@arg ctx this args $($as)*),)*
                ).into_js(ctx)
            }
        }
    };

    // $i - trait name (AsFunction or AsFunctionMut)
    // $s - self reference (& or &mut)
    // $t - argument type parameters
    // $tp - preceded type parameters
    // $ts - succeeded type parameters
    // $ap - preceded arg types
    // $as - succeeded arg types
    (@imp [Constructor $i:tt $($s:tt)*] $($t:ident)*; $($tp:ident)*: $([$($ap:tt)*]),*; $($ts:ident)*: $([$($as:tt)*]),*; ) => {
        #[cfg(feature = "classes")]
        impl<'js, C, F, $($tp,)* $($t,)* $($ts,)* R> $i<'js, (C, $($($ap)*,)* $($t,)* $($($as)*,)*), R> for Constructor<C, F>
        where
            C: ClassDef,
            F: Fn($($($ap)*,)* $($t,)* $($($as)*,)*) -> R,
            $($tp: FromJs<'js>,)*
            $($t: FromJs<'js>,)*
            $($ts: FromJs<'js>,)*
            R: IntoJs<'js>,
        {
            const LEN: u32 = 0 $(+ as_fn_impls!(@one $t))*;

            #[allow(unused_mut, unused)]
            fn call($($s)* self, ctx: Ctx<'js>, this: Value<'js>, mut args: ArgsIter<'js>) -> Result<Value<'js>> {
                let proto = match &this {
                    // called as constructor (with new keyword)
                    Value::Function(new_target) => new_target.get_prototype(),
                    // called as a function
                    _ => Class::<C>::prototype(ctx),
                }?;
                let res = self(
                    $(as_fn_impls!(@arg ctx this args $($ap)*),)*
                    $($t::from_js(ctx, args.next().ok_or_else(not_enough_args)?)?,)*
                    $(as_fn_impls!(@arg ctx this args $($as)*),)*
                ).into_js(ctx)?;
                if let Value::Object(obj) = &res {
                    obj.set_prototype(&proto)?;
                    Ok(res)
                } else {
                    Err(Error::IntoJs {
                        from: "value",
                        to: C::CLASS_NAME,
                        message: None,
                    })
                }
            }

            fn post<'js_>(ctx: Ctx<'js_>, func: &Function<'js_>) -> Result<()> {
                func.set_constructor(true);
                let proto = Class::<C>::prototype(ctx)?;
                func.set_prototype(&proto);
                Ok(())
            }
        }
    };

    // $f - closure kind (Fn or FnMut)
    // $i - trait name (AsFunction or AsFunctionMut)
    // $s - self reference (& or &mut)
    // $t - argument type parameters
    // $tp - preceded type parameters
    // $ts - succeeded type parameters
    // $ap - preceded arg types
    // $as - succeeded arg types
    (@imp [$f:tt $i:tt $($s:tt)*] $($t:ident)*; $($tp:ident)*: $([$($ap:tt)*]),*; $($ts:ident)*: $([$($as:tt)*]),*; ) => {
        impl<'js, F, $($tp,)* $($t,)* $($ts,)* R> $i<'js, ($($($ap)*,)* $($t,)* $($($as)*,)*), R> for F
        where
            F: $f($($($ap)*,)* $($t,)* $($($as)*,)*) -> R,
            $($tp: FromJs<'js>,)*
            $($t: FromJs<'js>,)*
            $($ts: FromJs<'js>,)*
            R: IntoJs<'js>,
        {
            const LEN: u32 = 0 $(+ as_fn_impls!(@one $t))*;

            #[allow(unused_mut, unused)]
            fn call($($s)* self, ctx: Ctx<'js>, this: Value<'js>, mut args: ArgsIter<'js>) -> Result<Value<'js>> {
                self(
                    $(as_fn_impls!(@arg ctx this args $($ap)*),)*
                    $($t::from_js(ctx, args.next().ok_or_else(not_enough_args)?)?,)*
                    $(as_fn_impls!(@arg ctx this args $($as)*),)*
                ).into_js(ctx)
            }
        }
    };

    (@arg $ctx:ident $this:ident $args:ident Ctx<'js>) => {
        $ctx
    };

    (@arg $ctx:ident $this:ident $args:ident This<T>) => {
        T::from_js($ctx, $this).map(This)?
    };

    (@arg $ctx:ident $this:ident $args:ident Args<X>) => {
        $args.map(|arg| X::from_js($ctx, arg))
             .collect::<Result<_>>().map(Args)?
    };

    (@one $($t:tt)*) => { 1 };
}

as_fn_impls! {
    ,
    A,
    A B,
    A B D,
    A B D E,
    A B D E G,
    A B D E G H,
}
#[cfg(feature = "max-args-7")]
as_fn_impls!(A B C D E G H I,);
#[cfg(feature = "max-args-8")]
as_fn_impls!(A B C D E G H I J,);
#[cfg(feature = "max-args-9")]
as_fn_impls!(A B C D E G H I J K,);
#[cfg(feature = "max-args-10")]
as_fn_impls!(A B C D E G H I J K L,);
#[cfg(feature = "max-args-11")]
as_fn_impls!(A B C D E G H I J K L M,);
#[cfg(feature = "max-args-12")]
as_fn_impls!(A B C D E G H I J K L M N,);
#[cfg(feature = "max-args-13")]
as_fn_impls!(A B C D E G H I J K L M N O,);
#[cfg(feature = "max-args-14")]
as_fn_impls!(A B C D E G H I J K L M N O P,);
#[cfg(feature = "max-args-15")]
as_fn_impls!(A B C D E G H I J K L M N O P U,);
#[cfg(feature = "max-args-16")]
as_fn_impls!(A B C D E G H I J K L M N O P U V,);

fn not_enough_args() -> Error {
    Error::FromJs {
        from: "args",
        to: "args",
        message: Some("Not enough arguments".into()),
    }
}