use mongodb::Cursor;
use serenity::futures::TryStreamExt;

use super::database::Weekend;

pub async fn filter_weekends(cur: &mut Cursor<Weekend>) -> Vec<Weekend> {
    let mut weekends: Vec<Weekend> = vec![];
    while let Some(weekend) = cur.try_next().await.unwrap_or(None) {
        if weekend.prolly_too_old() {
            continue;
        }
        weekends.push(weekend.clone());
    }
    weekends
}

pub fn best_weekend(weekends: &[Weekend]) -> Option<Weekend> {
    let mut best_match: Option<&Weekend> = None;
    for (_, weekend) in weekends.iter().enumerate() {
        if let Some(best) = best_match {
            if weekend.time_from_now() < best.time_from_now() {
                best_match = Some(weekend)
            }
        } else if best_match.is_none() {
            best_match = Some(weekend);
        }
    }
    best_match.cloned()
}
