//! Delivery vocabulary: statuses, who collects the money, how the driver is paid.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    ToSend,
    Assigned,
    PickedUp,
    Delivered,
    Failed,
    Returned,
    Cancelled,
}

impl Status {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ToSend => "to_send",
            Self::Assigned => "assigned",
            Self::PickedUp => "picked_up",
            Self::Delivered => "delivered",
            Self::Failed => "failed",
            Self::Returned => "returned",
            Self::Cancelled => "cancelled",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "to_send" => Self::ToSend,
            "assigned" => Self::Assigned,
            "picked_up" => Self::PickedUp,
            "delivered" => Self::Delivered,
            "failed" => Self::Failed,
            "returned" => Self::Returned,
            "cancelled" => Self::Cancelled,
            _ => return None,
        })
    }
}

/// Who takes the customer's money at the door.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Collector {
    Shop,
    Driver,
}

impl Collector {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Shop => "shop",
            Self::Driver => "driver",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "shop" => Some(Self::Shop),
            "driver" => Some(Self::Driver),
            _ => None,
        }
    }
}

/// How the driver gets their pay.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PayMethod {
    RetainFromCollection,
    PaidByShop,
}

impl PayMethod {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::RetainFromCollection => "retain_from_collection",
            Self::PaidByShop => "paid_by_shop",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "retain_from_collection" => Some(Self::RetainFromCollection),
            "paid_by_shop" => Some(Self::PaidByShop),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FailReason {
    CustomerUnavailable,
    AddressNotFound,
    CustomerRefused,
    UnreachableByPhone,
    Other,
}

impl FailReason {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::CustomerUnavailable => "customer_unavailable",
            Self::AddressNotFound => "address_not_found",
            Self::CustomerRefused => "customer_refused",
            Self::UnreachableByPhone => "unreachable_by_phone",
            Self::Other => "other",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "customer_unavailable" => Self::CustomerUnavailable,
            "address_not_found" => Self::AddressNotFound,
            "customer_refused" => Self::CustomerRefused,
            "unreachable_by_phone" => Self::UnreachableByPhone,
            "other" => Self::Other,
            _ => return None,
        })
    }
}
