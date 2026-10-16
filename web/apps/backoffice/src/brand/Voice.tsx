import { Icon, Table, type IconName } from "@obop/ui";
import { Block, DoDont } from "./kit.tsx";

const PRINCIPLES: [IconName, string, string][] = [
  [
    "eye",
    "Plain",
    "Say it the way you would to a colleague. Short words, short sentences, one idea each.",
  ],
  [
    "search",
    "Specific",
    "Name the thing and the number: Invoice INV-0042 posted, 3 leads deleted. Never Success.",
  ],
  [
    "check-circle",
    "Calm",
    "No exclamation marks, no blame, no drama. Things go wrong; we say what and what next.",
  ],
  [
    "shield-check",
    "Honest",
    "Say what the software does and does not do. No invented customers, numbers or promises.",
  ],
  ["help", "Helpful", "Every error says what happened and what to do about it, in that order."],
  ["user", "Human", "Sentence case. Verbs on buttons. The product is obop, lowercase, always."],
];

export function Voice() {
  return (
    <>
      <Block
        id="voice"
        title="Voice and tone"
        lead="obop sounds like a competent colleague: plain, specific, calm. The same voice in the product, the docs and the landing page; a little warmer when something goes well, a little more careful when something goes wrong."
      >
        <ul className="principles">
          {PRINCIPLES.map(([icon, name, text]) => (
            <li key={name}>
              <span className="principle-icon">
                <Icon name={icon} size={18} />
              </span>
              <h3>{name}</h3>
              <p>{text}</p>
            </li>
          ))}
        </ul>
      </Block>

      <Block
        id="voice-pairs"
        title="Say it like this"
        lead="Real strings from the product, before and after."
      >
        <div className="dodont-grid">
          <DoDont
            dont="Invalid input."
            doo="Enter an email like name@company.com."
            why="Errors say what to do, not only what is wrong."
          />
          <DoDont
            dont="Oops! Something went wrong!"
            doo="Could not save the lead. Check your connection and try again."
            why="Name the thing that failed. Skip the exclamation and the apology theatre."
          />
          <DoDont
            dont="No data."
            doo="No leads yet. Create the first one to start your pipeline."
            why="An empty state is a next step."
          />
          <DoDont
            dont="Are you sure?"
            doo="Delete 3 leads? This cannot be undone."
            why="Confirm with the consequence, and label the button with the action: Delete 3 leads."
          />
          <DoDont
            dont="Submit"
            doo="Create invoice"
            why="Buttons are verbs that say what happens."
          />
          <DoDont
            dont="Operation completed successfully."
            doo="Invoice INV-0042 posted."
            why="The toast confirms the specific result."
          />
          <DoDont
            dont="Action pending."
            doo="Waiting for a human to approve: post invoice INV-0042."
            why="Agents propose, humans approve. Say who is waiting for whom."
          />
          <DoDont
            dont="Your data is safe with us."
            doo="Personal data is masked unless you may see it."
            why="Describe the mechanism, not a feeling."
          />
          <DoDont
            dont="Authentication failed due to invalid credentials."
            doo="Invalid username or password."
            why="Short and in the user's words. Never reveal which half was wrong."
          />
        </div>
      </Block>

      <Block id="voice-words" title="Words and formats" lead="Pick one word per idea and keep it.">
        <div className="spec-row">
          <Table caption="Preferred words">
            <thead>
              <tr>
                <th scope="col">Use</th>
                <th scope="col">Not</th>
                <th scope="col">Why</th>
              </tr>
            </thead>
            <tbody>
              {[
                ["record", "entity, row, object", "People think in records: a lead, an invoice."],
                [
                  "app",
                  "module, package (in the UI)",
                  "Modules are the packaging; people open apps.",
                ],
                ["sign in / sign out", "log in / log out", "One verb pair everywhere."],
                ["delete / archive", "remove, destroy", "Delete cannot be undone. Archive can."],
                [
                  "agent",
                  "bot, AI, assistant",
                  "An agent has an owner and the owner's permissions at most.",
                ],
                [
                  "approve",
                  "confirm (for agent actions)",
                  "A human approves; the user confirms their own actions.",
                ],
                ["obop", "Obop, OBOP, the obop", "A name, lowercase, no article."],
              ].map(([a, b, c]) => (
                <tr key={a}>
                  <th scope="row">{a}</th>
                  <td>{b}</td>
                  <td>{c}</td>
                </tr>
              ))}
            </tbody>
          </Table>
          <Table caption="Formats">
            <thead>
              <tr>
                <th scope="col">Thing</th>
                <th scope="col">Format</th>
              </tr>
            </thead>
            <tbody>
              {[
                [
                  "Money",
                  "Locale symbol and grouping, two decimals, tabular: €1,250.00. Never floats; currencies are never added together silently.",
                ],
                [
                  "Dates",
                  "Locale short date in lists; 2026-10-16 in exports and the URL. Month names in full in headings.",
                ],
                ["Counts", "13 records, 1 record. Never (s)."],
                [
                  "Case",
                  "Sentence case for titles, buttons and menu items. UPPERCASE only for the tiny overline labels, set by CSS.",
                ],
                [
                  "Punctuation",
                  "No full stop in a button or a label; a full stop in a sentence. No exclamation marks.",
                ],
              ].map(([a, b]) => (
                <tr key={a}>
                  <th scope="row">{a}</th>
                  <td>{b}</td>
                </tr>
              ))}
            </tbody>
          </Table>
        </div>
      </Block>
    </>
  );
}
