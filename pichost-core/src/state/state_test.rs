use crate::state::*;

// 编译级契约：trait 必须可作 trait object
fn assert_queue_object(_q: &dyn Queue) {}
fn assert_blacklist_object(_b: &dyn Blacklist) {}
fn assert_rate_limiter_object(_r: &dyn RateLimiter) {}
fn assert_invite_object(_i: &dyn InviteStore) {}
fn assert_cache_object(_c: &dyn Cache) {}

#[test]
fn traits_are_object_safe() {
    assert_queue_object(&MockQueue);
    assert_blacklist_object(&MockBlacklist);
    assert_rate_limiter_object(&MockRateLimiter);
    assert_invite_object(&MockInviteStore);
    assert_cache_object(&MockCache);
}
