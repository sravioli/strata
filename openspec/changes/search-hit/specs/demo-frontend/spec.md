## ADDED Requirements

### Requirement: The search page shows each hit with its snippet

The page `/search` SHALL show one row for each hit of the response, in the order of the response, with the location and the snippet of the hit. It SHALL put the text of each segment in the page as text, never as HTML. It SHALL show a segment with `match` in bold.

#### Scenario: A hit with a matched word

- **GIVEN** a signed-in person on `/search`, and a document of the tenant with the word `bravo` on one page
- **WHEN** the person searches for `bravo`
- **THEN** the page shows one row for that page
- **AND** the row shows the words around `bravo`, with `bravo` in bold

#### Scenario: A snippet that holds markup

- **GIVEN** a hit whose segments hold the text `if x < y && a > b`
- **WHEN** the page shows the hit
- **THEN** the page shows the characters of the text, and it adds no element for them
