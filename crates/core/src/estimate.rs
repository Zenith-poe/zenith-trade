//! A price estimate from the listings of one search.
//!
//! There are no exchange rates yet, so only the currency most listings use is kept. Among
//! those prices, in ascending order, every price below half of their median is set apart
//! as too cheap to be real; the estimate is the median of the six cheapest that remain.
//! See [`suspicious`] for the rule.

/// How many of the cheapest remaining prices the estimate is taken from.
const CHEAPEST: usize = 6;

#[derive(Debug, Clone, PartialEq)]
pub struct Estimate {
    pub amount: f64,
    pub currency: String,
    /// How many prices the amount is the median of.
    pub from: usize,
    /// How many prices were set apart as too cheap.
    pub ignored: usize,
}

/// Which listings look like price fixing, one flag per entry of `prices`, in the same order.
///
/// Among the prices in the most used currency, a price is suspicious when it is under half the
/// median of the cheaper half of them (the cheapest `(n + 1) / 2`). Prices in another currency
/// are never flagged. The listings stay in the results; they only do not count for [`estimate`].
pub fn suspicious(prices: &[(f64, String)]) -> Vec<bool> {
    let Some(currency) = most_used_currency(prices) else {
        return Vec::new();
    };
    let mut amounts: Vec<f64> = prices
        .iter()
        .filter(|(_, c)| c == currency)
        .map(|(amount, _)| *amount)
        .collect();
    amounts.sort_by(f64::total_cmp);
    let cheaper = &amounts[..amounts.len().div_ceil(2)];
    let Some(median) = median(cheaper) else {
        return Vec::new();
    };
    let limit = median / 2.0;
    prices
        .iter()
        .map(|(amount, c)| c == currency && *amount < limit)
        .collect()
}

/// `prices` are the amount and the currency of each listing, in any order.
pub fn estimate(prices: &[(f64, String)]) -> Option<Estimate> {
    let currency = most_used_currency(prices)?;
    let mut amounts: Vec<f64> = prices
        .iter()
        .filter(|(_, listed_in)| listed_in == currency)
        .map(|(amount, _)| *amount)
        .collect();
    amounts.sort_by(f64::total_cmp);

    let too_cheap = median(&amounts[..amounts.len().div_ceil(2)])? / 2.0;
    let ignored = amounts.partition_point(|&amount| amount < too_cheap);
    let remaining = &amounts[ignored..];
    let cheapest = &remaining[..remaining.len().min(CHEAPEST)];

    Some(Estimate {
        amount: median(cheapest)?,
        currency: currency.to_string(),
        from: cheapest.len(),
        ignored,
    })
}

/// With equal counts, the currency that appears first in `prices`.
fn most_used_currency(prices: &[(f64, String)]) -> Option<&str> {
    let mut counts: Vec<(&str, usize)> = Vec::new();
    for (_, currency) in prices {
        match counts.iter_mut().find(|(seen, _)| seen == currency) {
            Some((_, count)) => *count += 1,
            None => counts.push((currency, 1)),
        }
    }
    let mut most: Option<(&str, usize)> = None;
    for (currency, count) in counts {
        if most.is_none_or(|(_, highest)| count > highest) {
            most = Some((currency, count));
        }
    }
    most.map(|(currency, _)| currency)
}

/// `sorted` is in ascending order. With an even count, the mean of the two middle values.
fn median(sorted: &[f64]) -> Option<f64> {
    let middle = sorted.len() / 2;
    match sorted.len() {
        0 => None,
        count if count % 2 == 1 => Some(sorted[middle]),
        _ => Some((sorted[middle - 1] + sorted[middle]) / 2.0),
    }
}
