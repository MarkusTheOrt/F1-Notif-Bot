use std::usize;

use chrono::prelude::*;
use super::database::Weekend;



pub fn get_best_weekend(weekends: &Vec<Weekend>) -> Option<&Weekend> {
    let mut best_match = -1;
    for (i, weekend) in weekends.iter().filter(|f| !f.done.unwrap_or(true)).enumerate() {
        if let Ok(time) = weekend.start.parse::<DateTime<Utc>>() {
            let diff = Utc::now().signed_duration_since(time);
            if diff.num_minutes() < best_match || best_match == -1{
                best_match = diff.num_minutes()
            }
        }
    }
    if best_match == -1 {
        return None;
    }
    weekends.get(best_match as usize)
}
