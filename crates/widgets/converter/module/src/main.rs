//! The converter's module: a number, two units, and the answer as you type.
//!
//! The whole widget is here. It asks its platform for nothing (the platform's
//! tests hold it to that), so it works the same on a read-only surface, and
//! the only thing it keeps is what the person typed: handed to the shell on
//! `suspend` when the panel is scrolled away, and taken back from `restored`
//! when it returns.

mod units;

use hlin_widget_module::{Widget, start};
use leptos::prelude::*;
use units::{QUANTITIES, Quantity, convert, quantity, written};

/// What the person has chosen and typed.
#[derive(Debug, Clone, PartialEq)]
struct Typed {
    quantity: &'static Quantity,
    from: String,
    to: String,
    value: String,
}

impl Typed {
    fn fresh(quantity: &'static Quantity) -> Self {
        Self {
            quantity,
            from: quantity.from.to_string(),
            to: quantity.to.to_string(),
            value: "1".to_string(),
        }
    }

    /// As the shell keeps it while the panel is away: four lines.
    fn kept(&self) -> Vec<u8> {
        [self.quantity.id, &self.from, &self.to, &self.value]
            .join("\n")
            .into_bytes()
    }

    /// Back from [`Typed::kept`], if it still makes sense.
    fn from_kept(bytes: &[u8]) -> Option<Self> {
        let text = std::str::from_utf8(bytes).ok()?;
        let mut parts = text.splitn(4, '\n');
        let quantity = quantity(parts.next()?)?;
        let (from, to) = (parts.next()?, parts.next()?);
        quantity.unit(from)?;
        quantity.unit(to)?;
        Some(Self {
            quantity,
            from: from.to_string(),
            to: to.to_string(),
            value: parts.next().unwrap_or_default().to_string(),
        })
    }
}

fn main() {
    start("converter", |widget: Widget| {
        let module = widget.module();
        let restored = module
            .restored()
            .and_then(|bytes| Typed::from_kept(&bytes))
            .unwrap_or_else(|| Typed::fresh(&QUANTITIES[0]));
        let typed = RwSignal::new(restored);
        module.on_suspend(move || Some(typed.with_untracked(Typed::kept)));

        let answer = move || {
            typed.with(|typed| convert(typed.quantity, &typed.value, &typed.from, &typed.to))
        };
        let units = move |chosen: fn(&Typed) -> &String| {
            typed.with(|typed| {
                typed
                    .quantity
                    .units
                    .iter()
                    .map(|unit| {
                        view! {
                            <option value=unit.id selected=chosen(typed) == unit.id>
                                {unit.name}
                            </option>
                        }
                    })
                    .collect_view()
            })
        };

        view! {
            <div class="w-row converter__quantities" role="tablist">
                {QUANTITIES
                    .iter()
                    .map(|quantity| {
                        let chosen = move || typed.with(|typed| typed.quantity.id == quantity.id);
                        view! {
                            <button
                                class="w-button converter__quantity"
                                class:w-button--quiet=move || !chosen()
                                role="tab"
                                aria-selected=move || chosen().to_string()
                                on:click=move |_| {
                                    if !chosen() {
                                        typed.set(Typed::fresh(quantity));
                                    }
                                }
                            >
                                {quantity.name}
                            </button>
                        }
                    })
                    .collect_view()}
            </div>
            <div class="converter__sum">
                <input
                    class="w-input converter__value"
                    aria-label="Value"
                    inputmode="decimal"
                    prop:value=move || typed.with(|typed| typed.value.clone())
                    on:input=move |event| typed.update(|typed| typed.value = event_target_value(&event))
                />
                <select
                    class="w-input"
                    aria-label="From"
                    on:change=move |event| typed.update(|typed| typed.from = event_target_value(&event))
                >
                    {move || units(|typed| &typed.from)}
                </select>
                <button
                    class="w-button w-button--quiet"
                    aria-label="Swap units"
                    on:click=move |_| typed.update(|typed| std::mem::swap(&mut typed.from, &mut typed.to))
                >
                    "⇄"
                </button>
                <select
                    class="w-input"
                    aria-label="To"
                    on:change=move |event| typed.update(|typed| typed.to = event_target_value(&event))
                >
                    {move || units(|typed| &typed.to)}
                </select>
            </div>
            {move || match answer() {
                Ok(value) => view! {
                    <p class="w-big converter__answer" data-value=written(value)>{written(value)}</p>
                }
                .into_any(),
                Err(words) if words.is_empty() => view! {
                    <p class="w-quiet">"Type a number to convert."</p>
                }
                .into_any(),
                Err(words) => view! { <p class="w-refusal">{words}</p> }.into_any(),
            }}
            <p class="w-quiet">"Worked out in your browser. Nothing is sent anywhere."</p>
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn what_was_typed_survives_being_scrolled_away_and_back() {
        let typed = Typed {
            quantity: quantity("temperature").unwrap(),
            from: "F".to_string(),
            to: "K".to_string(),
            value: "98.6".to_string(),
        };
        assert_eq!(Typed::from_kept(&typed.kept()), Some(typed));
    }

    #[test]
    fn kept_state_that_no_longer_makes_sense_is_ignored() {
        assert_eq!(Typed::from_kept(b"length\nkg\nm\n1"), None);
        assert_eq!(Typed::from_kept(b"weather\nC\nF\n1"), None);
        assert_eq!(Typed::from_kept(&[0xff, 0xfe]), None);
    }
}
