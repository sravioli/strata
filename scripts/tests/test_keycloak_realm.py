"""Tests of the realm file of the local Keycloak, dev/keycloak/strata-realm.json (tech design 0065).

Spec demo-frontend: TC-65-06. The test reads the file only, so it needs no Keycloak.
"""

import json
import pathlib
import unittest

REALM = pathlib.Path(__file__).resolve().parents[2] / "dev" / "keycloak" / "strata-realm.json"
SITE = "http://localhost:4321"


class KeycloakRealm(unittest.TestCase):
    def setUp(self):
        self.realm = json.loads(REALM.read_text())

    def test_tc_65_06_the_realm_file(self):
        """TC-65-06"""
        r = self.realm
        self.assertEqual(r["realm"], "strata")
        (client,) = [c for c in r["clients"] if c["clientId"] == "strata-web"]
        # Public, with the standard flow only.
        self.assertIs(client["publicClient"], True)
        self.assertIs(client["standardFlowEnabled"], True)
        for flow in ("implicitFlowEnabled", "directAccessGrantsEnabled", "serviceAccountsEnabled"):
            self.assertIs(client[flow], False, flow)
        self.assertEqual(client["attributes"]["pkce.code.challenge.method"], "S256")
        # Only the origin of the site.
        uris = client["redirectUris"] + client["attributes"]["post.logout.redirect.uris"].split("##")
        for uri in uris:
            self.assertTrue(uri.startswith(SITE + "/"), uri)
        self.assertEqual(client["webOrigins"], [SITE])
        # The claims of Strata: tenant as a string, and strata in aud.
        mappers = {m["protocolMapper"]: m["config"] for m in client["protocolMappers"]}
        tenant = mappers["oidc-usermodel-attribute-mapper"]
        self.assertEqual((tenant["user.attribute"], tenant["claim.name"]), ("tenant", "tenant"))
        self.assertEqual((tenant["jsonType.label"], tenant["multivalued"]), ("String", "false"))
        self.assertEqual(tenant["access.token.claim"], "true")
        self.assertEqual(mappers["oidc-audience-mapper"]["included.custom.audience"], "strata")
        # Fixed IDs, so a new container gives the same sub.
        users = r["users"]
        self.assertEqual({u["username"] for u in users}, {"alice@acme", "bob@acme", "alice@globex"})
        self.assertEqual(len({u["id"] for u in users}), len(users))
        for u in users:
            self.assertTrue(u.get("id"), u["username"])
            (value,) = u["attributes"]["tenant"]
            self.assertEqual(value, u["username"].split("@")[1])
        self.assertLessEqual(r["accessTokenLifespan"], 300)


if __name__ == "__main__":
    unittest.main()
