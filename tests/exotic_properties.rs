#![cfg(feature = "macro")]

use rquickjs::{
    class::{ExoticDefineResult, ExoticSetResult, PropertyDescriptor, PropertyName, Trace},
    Atom, Class, Context, Ctx, IntoJs, JsLifetime, Result, Runtime, Value,
};

#[derive(Trace, JsLifetime)]
#[rquickjs::class(exotic)]
struct Collection {
    value: i32,
}

#[rquickjs::methods]
impl Collection {
    fn item(&self) -> i32 {
        self.value
    }
}

#[rquickjs::exotic]
impl Collection {
    #[qjs(define_own_property)]
    fn define(
        &self,
        atom: Atom<'_>,
        _value: Value<'_>,
        is_data: bool,
    ) -> Result<ExoticDefineResult> {
        Ok(match atom.to_string()?.as_str() {
            "0" => ExoticDefineResult::Handled(false),
            "indexed" => ExoticDefineResult::Handled(is_data),
            _ => ExoticDefineResult::Fallthrough,
        })
    }

    #[qjs(get_own_property_names)]
    fn own_names<'js>(&self, ctx: &Ctx<'js>) -> Result<Vec<PropertyName<'js>>> {
        Ok(vec![PropertyName {
            atom: Atom::from_u32(ctx.clone(), 0)?,
            is_enumerable: true,
        }])
    }

    #[qjs(get_own_property)]
    fn own_property<'js>(
        &self,
        ctx: &Ctx<'js>,
        atom: Atom<'js>,
    ) -> Result<Option<PropertyDescriptor<'js>>> {
        if atom.to_string()? == "0" {
            Ok(Some(PropertyDescriptor::new_value(
                self.value.into_js(ctx)?,
                true,
                true,
                false,
            )))
        } else {
            Ok(None)
        }
    }
}

#[derive(Trace, JsLifetime)]
#[rquickjs::class(exotic)]
struct CustomGet {}

#[rquickjs::exotic]
impl CustomGet {
    #[qjs(get)]
    fn get(&self, atom: Atom<'_>) -> Result<Option<i32>> {
        Ok((atom.to_string()? == "custom").then_some(7))
    }
}

#[derive(Trace, JsLifetime)]
#[rquickjs::class(exotic)]
struct WritableCollection {
    indexed: i32,
}

#[rquickjs::exotic]
impl WritableCollection {
    #[qjs(set)]
    fn set<'js>(
        &mut self,
        ctx: &Ctx<'js>,
        atom: Atom<'js>,
        object: Value<'js>,
        receiver: Value<'js>,
        value: Value<'js>,
    ) -> Result<ExoticSetResult> {
        if atom.to_string()? == "detached" {
            ctx.eval::<(), _>("Object.setPrototypeOf(target, Object.prototype)")?;
            Ok(ExoticSetResult::Fallthrough)
        } else if atom.to_string()? == "defined" {
            ctx.eval::<(), _>(
                "Object.defineProperty(target, 'defined', { value: 5, writable: true, enumerable: true, configurable: true })",
            )?;
            Ok(ExoticSetResult::Fallthrough)
        } else if atom.to_string()? == "receiverOwn" {
            ctx.eval::<(), _>(
                "Object.defineProperty(collection, 'receiverOwn', { value: 5, writable: true, enumerable: true, configurable: true })",
            )?;
            Ok(ExoticSetResult::Fallthrough)
        } else if atom.to_string()? == "blocked" {
            Ok(ExoticSetResult::Handled(false))
        } else if atom.to_string()? == "named" && object != receiver {
            Ok(ExoticSetResult::FallthroughSkippingOwnProperty)
        } else if atom.to_string()? == "0" {
            if object != receiver {
                return Ok(ExoticSetResult::Fallthrough);
            }
            self.indexed = value.as_int().expect("integer test value");
            Ok(ExoticSetResult::Handled(true))
        } else {
            Ok(ExoticSetResult::Fallthrough)
        }
    }

    #[qjs(get_own_property)]
    fn own_property<'js>(
        &self,
        ctx: &Ctx<'js>,
        atom: Atom<'js>,
    ) -> Result<Option<PropertyDescriptor<'js>>> {
        if atom.to_string()? == "named" {
            Ok(Some(PropertyDescriptor::new_value(
                8.into_js(ctx)?,
                true,
                false,
                false,
            )))
        } else if atom.to_string()? == "0" {
            Ok(Some(PropertyDescriptor::new_value(
                self.indexed.into_js(ctx)?,
                true,
                true,
                true,
            )))
        } else {
            Ok(None)
        }
    }
}

#[test]
fn own_properties_do_not_hide_the_prototype_or_other_classes_hooks() -> Result<()> {
    let runtime = Runtime::new()?;
    let context = Context::full(&runtime)?;
    context.with(|ctx| {
        ctx.globals()
            .set("collection", Class::instance(ctx.clone(), Collection { value: 42 })?)?;
        ctx.globals()
            .set("custom", Class::instance(ctx.clone(), CustomGet {})?)?;

        assert!(ctx.eval::<bool, _>(
            "collection[0] === 42 && collection.item() === 42 && 'item' in collection && '0' in collection"
        )?);
        assert!(ctx.eval::<bool, _>(
            "collection.someProperty = 3; JSON.stringify(Object.getOwnPropertyNames(collection)) === '[\"0\",\"someProperty\"]'"
        )?);
        assert!(ctx.eval::<bool, _>(
            "Reflect.defineProperty(collection, 'ordinary', { value: 9 }) && collection.ordinary === 9 && !Reflect.defineProperty(collection, '0', { value: 7 }) && collection[0] === 42"
        )?);
        assert!(ctx.eval::<bool, _>(
            "!Reflect.defineProperty(collection, 'indexed', {}) && !Reflect.defineProperty(collection, 'indexed', { get() {} }) && Reflect.defineProperty(collection, 'indexed', { writable: true })"
        )?);
        assert!(ctx.eval::<bool, _>("custom.custom === 7")?);
        Ok(())
    })
}

#[test]
fn indexed_setter_preserves_ordinary_assignment() -> Result<()> {
    let runtime = Runtime::new()?;
    let context = Context::full(&runtime)?;
    context.with(|ctx| {
        ctx.globals().set(
            "collection",
            Class::instance(ctx.clone(), WritableCollection { indexed: 1 })?,
        )?;

        assert!(ctx.eval::<bool, _>(
            "collection[0] = 42; collection.custom = { value: 7 }; collection[0] === 42 && collection.custom.value === 7"
        )?);
        assert!(ctx.eval::<bool, _>(
            "const derived = Object.create(collection); derived[0] = 9; derived[0] === 9 && collection[0] === 42"
        )?);
        assert!(ctx.eval::<bool, _>(
            "derived.named = 3; derived.named === 3 && collection.named === 8"
        )?);
        assert!(ctx.eval::<bool, _>(
            "globalThis.target = Object.create(collection); target.detached = { value: 3 }; target.detached.value === 3 && Object.getPrototypeOf(target) === Object.prototype"
        )?);
        assert_eq!(ctx.eval::<String, _>(
            "Object.setPrototypeOf(target, collection); target.defined = 7; JSON.stringify([target.defined, Object.getOwnPropertyNames(target).filter(name => name === 'defined')])"
        )?, "[7,[\"defined\"]]");
        assert!(ctx.eval::<bool, _>(
            "collection.receiverOwn = 7; collection.receiverOwn === 7 && Object.getOwnPropertyNames(collection).filter(name => name === 'receiverOwn').length === 1"
        )?);
        assert!(ctx.eval::<bool, _>(
            "(function() { 'use strict'; try { collection.blocked = 1; return false; } catch (error) { return error instanceof TypeError; } })()"
        )?);
        Ok(())
    })
}
