use std::collections::HashSet;

use chrono::Datelike;

/// Configure retention of timestamps.
///
/// If either value is [None] every timestamp of the type will be kept.
#[derive(Copy, Clone, Debug, serde::Deserialize, serde::Serialize)]
pub struct RetentionConfig {
    /// Defines how many daily backups to keep.
    ///
    /// A daily backup is the first backup of the day.
    pub daily: Option<usize>,

    /// Defines how many weekly backups to keep.
    ///
    /// A weekly backup is the first backup of the week.
    pub weekly: Option<usize>,

    /// Defines how many monthly backups to keep.
    ///
    /// A monthly backup is the first backup of the monthly.
    pub monthly: Option<usize>,

    /// Defines how many quarterly backups to keep.
    ///
    /// A quarterly backup is the first backup of the quarter.
    pub quarterly: Option<usize>,

    /// Defines how many yearly backups to keep.
    ///
    /// A yearly backup is the first backup of the year.
    pub yearly: Option<usize>,
}

impl Default for RetentionConfig {
    fn default() -> Self {
        Self {
            daily: Some(10),
            weekly: Some(0),
            monthly: Some(10),
            quarterly: Some(0),
            yearly: Some(10),
        }
    }
}

#[derive(Clone, Debug)]
pub struct Retention {
    pub config: RetentionConfig,
    daily: HashSet<(i32, u32)>,
    weekly: HashSet<(i32, u32)>,
    monthly: HashSet<(i32, u32)>,
    quarterly: HashSet<(i32, u32)>,
    yearly: HashSet<i32>,
}

impl From<RetentionConfig> for Retention {
    fn from(config: RetentionConfig) -> Self {
        Self::new(config)
    }
}

impl Retention {
    pub fn new(config: RetentionConfig) -> Self {
        let daily = HashSet::new();
        let weekly = HashSet::new();
        let monthly = HashSet::new();
        let quarterly = HashSet::new();
        let yearly = HashSet::new();

        Self {
            config,
            daily,
            weekly,
            monthly,
            quarterly,
            yearly,
        }
    }

    /// Returns if the [Datelike] is to be retained.
    pub fn retain(&mut self, date: impl Datelike) -> bool {
        let Self {
            config,
            daily,
            weekly,
            monthly,
            quarterly,
            yearly,
        } = self;

        let new_daily = config
            .daily
            .is_none_or(|keep_daily| daily.len() < keep_daily)
            && {
                let daily_key = (date.year(), date.ordinal());
                daily.insert(daily_key)
            };

        let new_weekly = config
            .weekly
            .is_none_or(|keep_weekly| weekly.len() < keep_weekly)
            && {
                let weekly_key = (date.year(), date.iso_week().week());
                weekly.insert(weekly_key)
            };

        let new_monthly = config
            .monthly
            .is_none_or(|keep_monthly| monthly.len() < keep_monthly)
            && {
                let monthly_key = (date.year(), date.month());
                monthly.insert(monthly_key)
            };

        let new_quarterly = config
            .quarterly
            .is_none_or(|keep_quarterly| quarterly.len() < keep_quarterly)
            && {
                let quarterly_key = (date.year(), date.quarter());
                quarterly.insert(quarterly_key)
            };

        let new_yearly = config
            .yearly
            .is_none_or(|keep_yearly| yearly.len() < keep_yearly)
            && {
                let yearly_key = date.year();
                yearly.insert(yearly_key)
            };

        new_daily || new_weekly || new_monthly || new_quarterly || new_yearly
    }
}
