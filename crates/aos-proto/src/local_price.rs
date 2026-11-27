//! Bounded local arithmetic for self-contained French price questions.
//!
//! Exact sentence templates keep unrelated instructions on the normal route.
//! Amounts use cents and rates use basis points. Round the total once.

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PriceReply {
    Computed(String),
    Clarify,
}

const CLARIFY: &str =
    "Quels sont le prix initial, le pourcentage de remise et les frais exacts en euros ?";

impl PriceReply {
    pub fn into_text(self) -> String {
        match self {
            Self::Computed(text) => text,
            Self::Clarify => CLARIFY.into(),
        }
    }
}

pub fn price_reply(question: &str) -> Option<PriceReply> {
    if question.len() > 512 {
        return None;
    }
    let normalized = question
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
        .replace('’', "'")
        .replace(" euros", " €")
        .replace(" euro", " €");
    let request = normalized
        .strip_suffix(" réponds en une seule phrase en français avec le calcul.")
        .unwrap_or(&normalized);
    let body = [
        "un abonnement coûte ",
        "un article coûte ",
        "un produit coûte ",
    ]
    .into_iter()
    .find_map(|prefix| request.strip_prefix(prefix))?;
    let body = ["quel est le total ?", "quel est le total?"]
        .into_iter()
        .find_map(|suffix| body.strip_suffix(suffix))?
        .trim_end();
    let (amount, remainder) = body.split_once(". ")?;

    let (rate, fee, fee_first) = if let Some(rest) = remainder.strip_prefix("une remise de ") {
        let (rate, fee) = rest.split_once(" s'applique, puis on ajoute ")?;
        (rate, fee, false)
    } else {
        let rest = remainder.strip_prefix("on ajoute ")?;
        let (fee, rate) = rest.split_once(" de frais, puis une remise de ")?;
        (rate.strip_suffix(" s'applique.")?, fee, true)
    };
    let fee = if fee_first {
        fee
    } else {
        fee.strip_suffix(" de frais.")?
    };
    let amount = scaled_decimal(
        amount.trim_end_matches('€').trim_end(),
        amount.ends_with('€'),
    );
    let fee = scaled_decimal(fee.trim_end_matches('€').trim_end(), fee.ends_with('€'));
    let rate = scaled_decimal(rate.trim_end_matches('%').trim_end(), rate.ends_with('%'));
    let (Some(amount), Some(fee), Some(rate)) = (amount, fee, rate) else {
        return Some(PriceReply::Clarify);
    };
    if rate > 10_000 {
        return Some(PriceReply::Clarify);
    }
    let kept = 10_000 - rate;
    let amount_label = money(amount);
    let rate = money(rate);
    let fee_label = money(fee);
    let expression;
    let unrounded;
    if fee_first {
        unrounded = (amount + fee) * kept;
        expression = format!("({amount_label} € + {fee_label} €) × (1 − {rate} %)");
    } else {
        unrounded = amount * kept + fee * 10_000;
        expression = format!("{amount_label} € × (1 − {rate} %) + {fee_label} €");
    }
    let relation = if unrounded % 10_000 == 0 { "=" } else { "≈" };
    let total = money((unrounded + 5_000) / 10_000);
    Some(PriceReply::Computed(format!(
        "Le total est de {total} € ({expression} {relation} {total} €)."
    )))
}

fn scaled_decimal(text: &str, unit_present: bool) -> Option<u64> {
    if !unit_present {
        return None;
    }
    let (whole, fraction) = text.split_once([',', '.']).unwrap_or((text, ""));
    if whole.is_empty()
        || whole.len() > 9
        || !whole.bytes().all(|b| b.is_ascii_digit())
        || fraction.len() > 2
        || !fraction.bytes().all(|b| b.is_ascii_digit())
        || (fraction.is_empty() && whole != text)
    {
        return None;
    }

    let cents = match fraction.len() {
        0 => 0,
        1 => fraction.parse::<u64>().ok()? * 10,
        _ => fraction.parse().ok()?,
    };
    Some(whole.parse::<u64>().ok()? * 100 + cents)
}

fn money(value: u64) -> String {
    let whole = value / 100;
    let fraction = value % 100;
    if fraction == 0 {
        whole.to_string()
    } else {
        format!("{whole},{fraction:02}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn benchmark_prompt_uses_an_exact_single_sentence() {
        let question = "Un abonnement coûte 80 euros. Une remise de 25 % s'applique, puis on ajoute 5 euros de frais. Quel est le total ? Réponds en une seule phrase en français avec le calcul.";
        assert_eq!(
            price_reply(question).unwrap().into_text(),
            "Le total est de 65 € (80 € × (1 − 25 %) + 5 € = 65 €)."
        );
    }

    #[test]
    fn distinguishes_the_order_and_rounds_at_the_end() {
        let after = "Un article coûte 19,99 €. Une remise de 12,5 % s’applique, puis on ajoute 1,25 € de frais. Quel est le total ?";
        let before = "Un article coûte 19,99 €. On ajoute 1,25 € de frais, puis une remise de 12,5 % s’applique. Quel est le total ?";
        assert!(price_reply(after).unwrap().into_text().contains("18,74 €"));
        assert!(price_reply(before).unwrap().into_text().contains("18,59 €"));
        let half = "Un produit coûte 0,01 €. Une remise de 50 % s'applique, puis on ajoute 0 € de frais. Quel est le total ?";
        assert!(price_reply(half).unwrap().into_text().contains("≈ 0,01 €"));
    }

    #[test]
    fn asks_for_clarification_and_declines_additional_instructions() {
        let ambiguous = "Un abonnement coûte 80 €. Une remise de 25 % s'applique, puis on ajoute quelques euros de frais. Quel est le total ?";
        assert_eq!(price_reply(ambiguous), Some(PriceReply::Clarify));
        let multi_step = "Un abonnement coûte 80 €. Une remise de 25 % s'applique, puis on ajoute 5 € de frais. Quel est le total ? Cherche ensuite des offres.";
        assert_eq!(price_reply(multi_step), None);
    }
}
