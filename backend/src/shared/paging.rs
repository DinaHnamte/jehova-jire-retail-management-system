//! Page-number paging for list endpoints. Out-of-range values are clamped.
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Paging {
    pub page: i64,
    pub per_page: i64,
}

impl Paging {
    pub const DEFAULT_PER_PAGE: i64 = 25;
    pub const MAX_PER_PAGE: i64 = 100;

    pub fn from_query(page: Option<i64>, per_page: Option<i64>) -> Self {
        Self {
            page: page.unwrap_or(1).max(1),
            per_page: per_page
                .unwrap_or(Self::DEFAULT_PER_PAGE)
                .clamp(1, Self::MAX_PER_PAGE),
        }
    }

    pub fn offset(&self) -> i64 {
        (self.page - 1).saturating_mul(self.per_page)
    }
}

#[derive(Debug, Serialize)]
pub struct Page<T> {
    pub items: Vec<T>,
    pub page: i64,
    pub per_page: i64,
    pub total: i64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paging_clamps() {
        assert_eq!(
            Paging::from_query(None, None),
            Paging {
                page: 1,
                per_page: 25
            }
        );
        assert_eq!(
            Paging::from_query(Some(0), Some(0)),
            Paging {
                page: 1,
                per_page: 1
            }
        );
        assert_eq!(
            Paging::from_query(Some(-5), Some(1000)),
            Paging {
                page: 1,
                per_page: 100
            }
        );
        assert_eq!(Paging::from_query(Some(3), Some(25)).offset(), 50);
    }
}
