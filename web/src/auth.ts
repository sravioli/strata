// The sign-in of the site (tech design 0065): the code flow with PKCE S256 of oidc-client-ts, as the
// public client strata-web of the local Keycloak. The token is in memory and in sessionStorage, so
// it ends with the tab. oidc-client-ts renews it with the refresh token before it ends.
import { UserManager, WebStorageStateStore, type User } from "oidc-client-ts";

export const ISSUER = "http://localhost:8180/realms/strata";

const manager = new UserManager({
  authority: ISSUER,
  client_id: "strata-web",
  redirect_uri: `${location.origin}/`,
  post_logout_redirect_uri: `${location.origin}/`,
  scope: "openid",
  userStore: new WebStorageStateStore({ store: sessionStorage }),
  automaticSilentRenew: true,
});

export interface Person {
  user: User;
  tenant: string;
  sub: string;
}

/** The signed-in person, or null. On the redirect back from Keycloak, it ends the sign-in first. */
export async function person(): Promise<Person | null> {
  const params = new URLSearchParams(location.search);
  if (params.has("code") && params.has("state")) {
    await manager.signinRedirectCallback();
    history.replaceState(null, "", location.pathname);
  }
  const user = await manager.getUser();
  if (!user || user.expired) return null;
  return { user, tenant: String(user.profile.tenant ?? ""), sub: user.profile.sub };
}

export function signIn(): Promise<void> {
  return manager.signinRedirect();
}

export function signOut(): Promise<void> {
  return manager.signoutRedirect();
}

/** The access token of now: the renewal can replace it. */
export async function accessToken(): Promise<string | null> {
  const user = await manager.getUser();
  return user && !user.expired ? user.access_token : null;
}
