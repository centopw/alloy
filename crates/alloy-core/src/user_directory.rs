//! Homeserver user-directory search (`POST /_matrix/client/v3/user_directory/search`).

use crate::room_search::{clamp_limit, validate_query};
use crate::{AlloyClient, AlloyError, AlloyResult};
use alloy_protocol::{UserId, UserSearchResultDto};
use matrix_sdk::ruma::events::room::member::MembershipState;

/// One page of user-directory results.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UserSearchPage {
    pub results: Vec<UserSearchResultDto>,
    /// The homeserver truncated the list at the requested limit.
    pub limited: bool,
}

/// Whether this membership means the user is already reachable in the room.
fn reachable_membership(state: &MembershipState) -> bool {
    matches!(state, MembershipState::Join | MembershipState::Invite)
}

/// Flags a directory result that already shares a joined room with the account.
///
/// Membership comes from the local state store, so no extra request is made. A failed lookup
/// only clears the flag for that room instead of failing the whole search.
async fn is_in_joined_room(rooms: &[matrix_sdk::Room], user_id: &matrix_sdk::ruma::UserId) -> bool {
    for room in rooms {
        match room.get_member_no_sync(user_id).await {
            Ok(Some(member)) if reachable_membership(member.membership()) => return true,
            Ok(_) => {}
            Err(error) => {
                tracing::warn!(
                    %error,
                    room = %room.room_id(),
                    "could not read membership for a user search result"
                );
            }
        }
    }
    false
}

impl AlloyClient {
    /// Searches the homeserver user directory by user id and display name.
    ///
    /// `query` must not be blank and is sent as given; `limit` is clamped to 1..=50. The server's
    /// `limited` flag is passed through unchanged: it is true when more matches exist than the
    /// returned page holds.
    pub async fn search_users(&self, query: &str, limit: u32) -> AlloyResult<UserSearchPage> {
        validate_query(query)?;
        let response = self
            .client
            .search_users(query, u64::from(clamp_limit(limit)))
            .await
            .map_err(|e| AlloyError::MatrixSdk(e.to_string()))?;

        let rooms = self.client.joined_rooms();
        let mut results = Vec::with_capacity(response.results.len());
        for user in response.results {
            let already_in_room = is_in_joined_room(&rooms, &user.user_id).await;
            results.push(UserSearchResultDto {
                user_id: UserId(user.user_id.to_string()),
                display_name: user.display_name,
                avatar_url: user.avatar_url.map(|uri| uri.to_string()),
                already_in_room,
            });
        }

        Ok(UserSearchPage {
            results,
            limited: response.limited,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn joined_and_invited_members_count_as_reachable() {
        assert!(reachable_membership(&MembershipState::Join));
        assert!(reachable_membership(&MembershipState::Invite));
    }

    #[test]
    fn left_banned_and_knocked_members_do_not_count() {
        assert!(!reachable_membership(&MembershipState::Leave));
        assert!(!reachable_membership(&MembershipState::Ban));
        assert!(!reachable_membership(&MembershipState::Knock));
    }
}
