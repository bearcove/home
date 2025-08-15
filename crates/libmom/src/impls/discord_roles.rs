use std::sync::Arc;

use mom_types::AllUsers;

use crate::impls::MomTenantState;

async fn synchronize_discord_roles(ts: &MomTenantState, users: Arc<AllUsers>) {
    // first, fetch first guild the bot is in

    // then, fetch all roles for this server

    // map roles to `FasterthanlimeTier`, see @credentials/src/lib.rs

    // fetch all members of the servers

    // go through them one by one, compare the roles they do have
    // vs the roles they should have. we're not touching roles outside
    // of the roles mapped earlier: usually we'd have Bronze, Silver, Gold,
    // here's an example of the roles answer we got for the test server:

    /*
    "roles": [
        {
          "id": "1405484335045279796",
          "name": "@everyone",
          "color": 0,
          "hoist": false,
          "icon": null,
          "unicode_emoji": null,
          "position": 0,
          "permissions": "2248473465835073",
          "managed": false,
          "mentionable": false
        },
        {
          "id": "1405498658887041045",
          "name": "Bronze",
          "color": 15105570,
          "hoist": false,
          "icon": null,
          "unicode_emoji": null,
          "position": 1,
          "permissions": "0",
          "managed": false,
          "mentionable": false
        },
        {
          "id": "1405498704626188339",
          "name": "Silver",
          "color": 0,
          "hoist": false,
          "icon": null,
          "unicode_emoji": null,
          "position": 2,
          "permissions": "0",
          "managed": false,
          "mentionable": false
        },
        {
          "id": "1405498725241061496",
          "name": "Gold",
          "color": 15844367,
          "hoist": false,
          "icon": null,
          "unicode_emoji": null,
          "position": 3,
          "permissions": "0",
          "managed": false,
          "mentionable": false
        },
        {
          "id": "1405498929235099768",
          "name": "Admin",
          "color": 15277667,
          "hoist": false,
          "icon": null,
          "unicode_emoji": null,
          "position": 4,
          "permissions": "0",
          "managed": false,
          "mentionable": false
        },
        {
          "id": "1405510611114262538",
          "name": "fasterthanlime-dev",
          "color": 0,
          "hoist": false,
          "icon": null,
          "unicode_emoji": null,
          "position": 1,
          "permissions": "268435456",
          "managed": true,
          "mentionable": false
        }
      ]
      */

    // So we're building maybe a hashmap with `id => bool`, like if
    // they have silver, then the map is:
    // { gold_id => false, silver_id => true, bronze_id => false }
    // and we make individual calls to add_guild_member_role and
    // remove_guild_member_role as needed.
    // but we make sure to NOT TOUCH other roles like Admin etc.
}
