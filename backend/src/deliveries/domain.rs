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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Assign,
    Reassign,
    PickUp,
    Deliver,
    Fail,
    Retry,
    CheckIn,
    Cancel,
}

/// The status an action leads to, or `Err(from)` (the current status) if it isn't allowed.
pub fn next_status(from: Status, action: Action) -> Result<Status, Status> {
    use Action::*;
    use Status::*;
    match (from, action) {
        (ToSend, Assign) | (Assigned, Reassign) | (Failed, Retry) => Ok(Assigned),
        (Assigned, PickUp) => Ok(PickedUp),
        (PickedUp, Deliver) => Ok(Delivered),
        (PickedUp, Fail) => Ok(Failed),
        (Failed, CheckIn) => Ok(Returned),
        (ToSend | Assigned, Cancel) => Ok(Cancelled),
        _ => Err(from),
    }
}

/// On-screen wording for a status.
pub fn status_label(s: Status) -> &'static str {
    match s {
        Status::ToSend => "To send",
        Status::Assigned => "Assigned",
        Status::PickedUp => "Out for delivery",
        Status::Delivered => "Delivered",
        Status::Failed => "Couldn't deliver",
        Status::Returned => "Returned to shop",
        Status::Cancelled => "Cancelled",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use Action::*;
    use Status::*;

    const ALL_STATUS: [Status; 7] = [
        ToSend, Assigned, PickedUp, Delivered, Failed, Returned, Cancelled,
    ];
    const ALL_ACTIONS: [Action; 8] = [
        Assign, Reassign, PickUp, Deliver, Fail, Retry, CheckIn, Cancel,
    ];
    const ALLOWED: [(Status, Action, Status); 9] = [
        (ToSend, Assign, Assigned),
        (Assigned, Reassign, Assigned),
        (Assigned, PickUp, PickedUp),
        (PickedUp, Deliver, Delivered),
        (PickedUp, Fail, Failed),
        (Failed, Retry, Assigned),
        (Failed, CheckIn, Returned),
        (ToSend, Cancel, Cancelled),
        (Assigned, Cancel, Cancelled),
    ];

    #[test]
    fn allowed_transitions() {
        for (from, action, to) in ALLOWED {
            assert_eq!(next_status(from, action), Ok(to), "{from:?} {action:?}");
        }
    }

    #[test]
    fn everything_else_is_rejected() {
        for from in ALL_STATUS {
            for action in ALL_ACTIONS {
                if ALLOWED.iter().any(|(f, a, _)| *f == from && *a == action) {
                    continue;
                }
                assert_eq!(next_status(from, action), Err(from), "{from:?} {action:?}");
            }
        }
    }

    #[test]
    fn labels_match_copy_table() {
        assert_eq!(status_label(PickedUp), "Out for delivery");
        assert_eq!(status_label(Failed), "Couldn't deliver");
        assert_eq!(status_label(Returned), "Returned to shop");
    }
}
