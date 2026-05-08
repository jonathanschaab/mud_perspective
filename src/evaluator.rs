use std::borrow::Cow;
use std::collections::HashMap;

use crate::models::{RenderContext, TemplateEntity};
use crate::parser::{Condition, ConditionValue, TAG_PROPERTY_SEP, TagSegment};

#[inline]
pub(crate) fn get_entity<'a>(
    ctx: &'a RenderContext,
    key: &str,
) -> Result<&'a dyn TemplateEntity, String> {
    // 1. Try exact match first (e.g., "source")
    if let Some(entity) = ctx.entities.get(key).copied() {
        return Ok(entity);
    }

    // 2. Try dot notation traversal (e.g., "source.left_arm.weapon")
    if let Some((root_key, remainder)) = key.split_once(TAG_PROPERTY_SEP)
        && let Some(mut current) = ctx.entities.get(root_key).copied()
    {
        let mut current_path = root_key;
        for prop in remainder.split(TAG_PROPERTY_SEP) {
            current = current
                .get_property(prop)
                .ok_or_else(|| format!("Missing property '{prop}' on entity '{current_path}'"))?;
            // Accumulate the traversed path slice by extending it over the dot and property
            let next_len = current_path.len() + 1 + prop.len();
            current_path = key.get(..next_len).unwrap_or(current_path);
        }
        return Ok(current);
    }

    tracing::error!("Failed to render template: Missing entity for key '{key}'");
    Err(format!("Missing entity for key: {key}"))
}

#[inline]
pub(crate) fn resolve_entity_property<'a>(
    ctx: &'a RenderContext,
    key: &str,
    fallback: Option<&str>,
    pre_resolved: &HashMap<&str, &'a dyn TemplateEntity>,
) -> Result<Option<Cow<'a, str>>, String> {
    if let Some((ent, prop)) = key.rsplit_once('.') {
        let root_key = ent.split_once('.').map_or(ent, |(r, _)| r);
        if ctx.entities.contains_key(root_key) {
            if let Ok(entity) = pre_resolved
                .get(ent)
                .copied()
                .map_or_else(|| get_entity(ctx, ent), Ok)
                && let Some(val) = entity.get_string_property(prop)
            {
                return Ok(Some(val));
            }
            if let Some(fb) = fallback {
                return Ok(Some(Cow::Owned(fb.to_string())));
            }
            tracing::error!("Missing string property '{prop}' on entity '{ent}'");
            return Err(format!(
                "Missing string property '{prop}' on entity '{ent}'"
            ));
        }
    }
    Ok(None)
}

#[inline]
pub(crate) fn resolve_tag_segment<'a>(
    ctx: &'a RenderContext,
    segment: &'a TagSegment,
    pre_resolved: &HashMap<&str, &'a dyn TemplateEntity>,
) -> Result<Cow<'a, str>, String> {
    match segment {
        TagSegment::Literal(s) => Ok(Cow::Borrowed(s.as_str())),
        TagSegment::Variable { key: k, fallback } => {
            if let Some(val) = resolve_entity_property(ctx, k, fallback.as_deref(), pre_resolved)? {
                return Ok(val);
            }
            if let Some(values) = ctx.variables.get(k.as_str()) {
                match values.as_slice() {
                    [] => Ok(Cow::Borrowed("")),
                    [single] => Ok(Cow::Borrowed(single.as_str())),
                    _ => Ok(Cow::Owned(values.join(" "))),
                }
            } else {
                if let Some(fb) = fallback {
                    return Ok(Cow::Owned(fb.clone()));
                }
                tracing::error!("Missing dynamic variable for key '{k}'");
                Err(format!("Missing variable for key: {k}"))
            }
        }
    }
}

pub(crate) fn evaluate_condition_value_bool(
    ctx: &RenderContext,
    val: &ConditionValue,
    pre_resolved: &HashMap<&str, &dyn TemplateEntity>,
) -> bool {
    match val {
        ConditionValue::Literal(s) => !s.is_empty() && !s.eq_ignore_ascii_case("false") && s != "0",
        ConditionValue::Number(n) => *n != 0.0,
        ConditionValue::Variable(var) => {
            ctx.variables
                .get(var.as_str())
                .is_some_and(|v| match v.as_slice() {
                    [] => false,
                    [f] => !f.is_empty() && !f.eq_ignore_ascii_case("false") && f != "0",
                    _ => true,
                })
        }
        ConditionValue::EntityProperty(ent, prop) => {
            if let Ok(entity) = pre_resolved
                .get(ent.as_str())
                .copied()
                .map_or_else(|| get_entity(ctx, ent), Ok)
            {
                entity.check_condition(prop)
            } else {
                false
            }
        }
    }
}

pub(crate) fn evaluate_condition_value_string<'a>(
    ctx: &'a RenderContext,
    val: &'a ConditionValue,
    pre_resolved: &HashMap<&str, &'a dyn TemplateEntity>,
) -> Option<Cow<'a, str>> {
    match val {
        ConditionValue::Literal(s) => Some(Cow::Borrowed(s.as_str())),
        ConditionValue::Number(n) => Some(Cow::Owned(n.to_string())),
        ConditionValue::Variable(var) => {
            ctx.variables.get(var.as_str()).map(|v| match v.as_slice() {
                [] => Cow::Borrowed(""),
                [single] => Cow::Borrowed(single.as_str()),
                _ => Cow::Owned(v.join(" ")),
            })
        }
        ConditionValue::EntityProperty(ent, prop) => {
            if let Ok(entity) = pre_resolved
                .get(ent.as_str())
                .copied()
                .map_or_else(|| get_entity(ctx, ent), Ok)
            {
                entity.get_string_property(prop)
            } else {
                None
            }
        }
    }
}

pub(crate) fn get_numeric_value(
    ctx: &RenderContext,
    val: &ConditionValue,
    pre_resolved: &HashMap<&str, &dyn TemplateEntity>,
) -> Option<f64> {
    if let ConditionValue::Number(n) = val {
        Some(*n)
    } else {
        let s_opt = evaluate_condition_value_string(ctx, val, pre_resolved);
        s_opt.and_then(|s| s.parse::<f64>().ok())
    }
}

pub(crate) fn evaluate_numeric(
    ctx: &RenderContext,
    left: &ConditionValue,
    right: &ConditionValue,
    pre_resolved: &HashMap<&str, &dyn TemplateEntity>,
    op: impl Fn(f64, f64) -> bool,
) -> bool {
    let left_num = get_numeric_value(ctx, left, pre_resolved);
    let right_num = get_numeric_value(ctx, right, pre_resolved);
    if let (Some(l), Some(r)) = (left_num, right_num) {
        op(l, r)
    } else {
        false
    }
}

pub(crate) fn evaluate_condition(
    ctx: &RenderContext,
    condition: &Condition,
    pre_resolved: &HashMap<&str, &dyn TemplateEntity>,
) -> bool {
    match condition {
        Condition::Value(val) => evaluate_condition_value_bool(ctx, val, pre_resolved),
        Condition::Not(inner) => !evaluate_condition(ctx, inner, pre_resolved),
        Condition::And(left, right) => {
            evaluate_condition(ctx, left, pre_resolved)
                && evaluate_condition(ctx, right, pre_resolved)
        }
        Condition::Or(left, right) => {
            evaluate_condition(ctx, left, pre_resolved)
                || evaluate_condition(ctx, right, pre_resolved)
        }
        Condition::Eq(left, right) => {
            let left_str = evaluate_condition_value_string(ctx, left, pre_resolved);
            let right_str = evaluate_condition_value_string(ctx, right, pre_resolved);
            left_str == right_str
        }
        Condition::NotEq(left, right) => {
            let left_str = evaluate_condition_value_string(ctx, left, pre_resolved);
            let right_str = evaluate_condition_value_string(ctx, right, pre_resolved);
            left_str != right_str
        }
        Condition::Gt(left, right) => {
            evaluate_numeric(ctx, left, right, pre_resolved, |l, r| l > r)
        }
        Condition::Lt(left, right) => {
            evaluate_numeric(ctx, left, right, pre_resolved, |l, r| l < r)
        }
        Condition::GtEq(left, right) => {
            evaluate_numeric(ctx, left, right, pre_resolved, |l, r| l >= r)
        }
        Condition::LtEq(left, right) => {
            evaluate_numeric(ctx, left, right, pre_resolved, |l, r| l <= r)
        }
    }
}
