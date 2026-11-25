// TC-65-07: from sign-in to search in the browser, for two tenants (spec demo-frontend).
// Run make dev and make web first, then `npm run test:e2e` in web/.
import { expect, test, type Page } from "@playwright/test";

const FIXTURE = new URL("../../crates/strata-tests/fixtures/two-pages.pdf", import.meta.url).pathname;
/** A word of page 2 of the fixture. */
const WORD = "bravo";

/** The sign-in through the login page of Keycloak, with the passwords of dev/README.md. */
async function signIn(page: Page, user: string, password: string) {
  await page.goto("/");
  await page.getByRole("button", { name: "Sign in with Keycloak" }).click();
  await page.locator("#username").fill(user);
  await page.locator("#password").fill(password);
  await page.locator("#kc-login").click();
  await expect(page.getByRole("status")).toContainText("Signed in. Tenant: ");
}

async function search(page: Page) {
  await page.goto("/search");
  await page.getByLabel("Words").fill(WORD);
  await page.getByRole("button", { name: "Search" }).click();
  await expect(page.getByRole("status")).toHaveText(/hits?\.$/);
  return page.locator("#hits li").allTextContents();
}

test("TC-65-07 from sign-in to search in the browser", async ({ browser }) => {
  const acme = await (await browser.newContext()).newPage();
  await signIn(acme, "alice@acme", "alice-acme-dev");

  await acme.goto("/upload");
  await acme.locator("#new input[type=file]").setInputFiles(FIXTURE);
  await acme.getByRole("button", { name: "Upload", exact: true }).click();
  const uploaded = acme.getByRole("status");
  await expect(uploaded).toContainText("Uploaded two-pages.pdf");
  const document_id = (await uploaded.textContent())!.match(/document ([0-9a-f-]{36})/)![1];

  await acme.goto("/documents");
  const row = acme.locator("tr", { hasText: document_id });
  await expect(row.locator("td[data-state]")).toHaveAttribute("data-state", /^(ready|failed|superseded)$/, {
    timeout: 60_000,
  });
  await expect(row.locator("td[data-state]")).toHaveAttribute("data-state", "ready");
  expect((await search(acme)).some((h) => h.includes(document_id))).toBe(true);
  // The hit of the document shows the searched word in bold, in its snippet.
  const hit = acme.locator("#hits li", { hasText: document_id });
  await expect(hit.locator("blockquote strong").first()).toHaveText(WORD);

  const globex = await (await browser.newContext()).newPage();
  await signIn(globex, "alice@globex", "alice-globex-dev");
  expect(await search(globex)).toEqual([]);
});
