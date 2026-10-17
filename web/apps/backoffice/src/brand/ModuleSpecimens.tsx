import {
  CoreBadge,
  LicenseBadge,
  MaturityBadge,
  SourceBadge,
  StatusChip,
} from "../modules/badges.tsx";
import { DependencyTree, CoreRequirement } from "../modules/DependencyTree.tsx";
import { BlockerList, PlanSteps } from "../modules/PlanDialog.tsx";
import type { DepStatus, TreeNode } from "../modules/status.ts";
import type { Plan } from "../modules/types.ts";
import { Specimen } from "./kit.tsx";

const STATUSES: DepStatus[] = [
  "satisfied",
  "builtin",
  "will_install",
  "will_upgrade",
  "missing",
  "conflict",
];

const TREE: TreeNode[] = [
  {
    id: "hr",
    name: "Human Resources",
    requirement: ">=0.2, <1",
    status: "satisfied",
    installedVersion: "0.2.0",
    availableVersion: "0.2.0",
    cycle: false,
    children: [
      {
        id: "base",
        name: "Contacts",
        requirement: "^0.2",
        status: "builtin",
        installedVersion: "0.2.0",
        availableVersion: null,
        cycle: false,
        children: [],
      },
    ],
  },
  {
    id: "projects",
    name: "Projects",
    requirement: ">=0.3, <1",
    status: "will_install",
    installedVersion: null,
    availableVersion: "0.3.0",
    cycle: false,
    children: [],
  },
  {
    id: "legacy",
    name: "Legacy sync",
    requirement: "<1",
    status: "conflict",
    installedVersion: "2.1.0",
    availableVersion: null,
    cycle: false,
    children: [],
  },
];

const PLAN: Plan = {
  ok: true,
  root: "timesheets",
  core_version: "0.2.0",
  warnings: [],
  blockers: [],
  steps: [
    {
      id: "projects",
      name: "Projects",
      version: "0.3.0",
      action: "install",
      from_version: null,
      license: "AGPL-3.0-or-later",
      license_kind: "spdx",
      reason: "needed by timesheets (projects >=0.3, <1)",
      requested: false,
      source: "marketplace",
      requires_core: "^0.2",
      author: "SpaceCorps",
      summary: "",
      dependencies: [],
    },
    {
      id: "timesheets",
      name: "Timesheets",
      version: "0.2.0",
      action: "install",
      from_version: null,
      license: "AGPL-3.0-or-later",
      license_kind: "spdx",
      reason: "requested",
      requested: true,
      source: "marketplace",
      requires_core: "^0.2",
      author: "SpaceCorps",
      summary: "",
      dependencies: [
        {
          id: "projects",
          name: "Projects",
          requirement: ">=0.3, <1",
          kind: "package",
          installed_version: null,
          satisfied: false,
          available_version: "0.3.0",
          available_satisfies: true,
          optional: false,
        },
      ],
    },
  ],
};

const nameOf = (id: string) => id.charAt(0).toUpperCase() + id.slice(1);

/** Specimens of the module catalogue pieces (D55): badges, the dependency tree and the install plan. */
export function ModuleSpecimens() {
  return (
    <>
      <Specimen
        title="Module badges"
        note="A module card says four things at a glance: its licence, how settled it is, whether it works with the running core, and where it comes from. Red is reserved for a core it cannot run on."
        props={[
          [
            "LicenseBadge row",
            "{ license, license_kind }",
            "required",
            "An SPDX expression as written. A proprietary or custom licence gets the warning tone and an icon.",
          ],
          [
            "MaturityBadge maturity",
            '"stable" | "beta" | "experimental"',
            "required",
            "Nothing is drawn when a module declares none.",
          ],
          [
            "CoreBadge row, core",
            "{ compatible, requires_core }, string",
            "required",
            'Green "Works with core 0.2", or red with both versions: "Needs core ≥0.3, you run 0.2".',
          ],
          [
            "StatusChip status",
            "DepStatus",
            "required",
            "satisfied, builtin, will_install, will_upgrade, missing or conflict; icon and word, never colour alone.",
          ],
        ]}
      >
        <div className="label-grid">
          <div className="chip-row">
            <LicenseBadge row={{ license: "AGPL-3.0-or-later", license_kind: "spdx" }} />
            <LicenseBadge row={{ license: "Apache-2.0", license_kind: "spdx" }} />
            <LicenseBadge row={{ license: "proprietary", license_kind: "proprietary" }} />
            <LicenseBadge row={{ license: "LicenseRef-Acme", license_kind: "custom" }} />
            <LicenseBadge row={{ license: "", license_kind: "none" }} />
          </div>
          <div className="chip-row">
            <MaturityBadge maturity="stable" />
            <MaturityBadge maturity="beta" />
            <MaturityBadge maturity="experimental" />
            <SourceBadge source="builtin" />
            <SourceBadge source="marketplace" />
            <SourceBadge source="upload" />
          </div>
          <div className="chip-row">
            <CoreBadge row={{ compatible: true, requires_core: "^0.2" }} core="0.2.0" />
            <CoreBadge row={{ compatible: false, requires_core: ">=0.3" }} core="0.2.0" />
          </div>
          <div className="chip-row">
            {STATUSES.map((s) => (
              <StatusChip key={s} status={s} />
            ))}
          </div>
        </div>
      </Specimen>

      <Specimen
        title="Dependency tree"
        note="Nested lists with connecting lines: the core requirement at the root, then each dependency with the range its parent needs, the version installed here and a status chip."
        wide
        props={[
          [
            "DependencyTree nodes",
            "TreeNode[]",
            "required",
            "Built with buildTree from the dependencies the server sends; a module that repeats an ancestor stops there.",
          ],
          [
            "DependencyTree hrefFor",
            "(id) => string | null",
            "none",
            "Makes a module name a link to its own panel.",
          ],
        ]}
      >
        <div className="stack">
          <CoreRequirement name="Timesheets" requirement="^0.2" core="0.2.0" compatible />
          <DependencyTree nodes={TREE} label="Dependencies of Timesheets" />
        </div>
      </Specimen>

      <Specimen
        title="Install plan"
        note="What the plan dialog lists before anyone confirms: the steps in the order they run, with versions and licences, then anything that blocks them."
        wide
        props={[
          [
            "PlanSteps plan, nameOf",
            "Plan, (id) => string",
            "required",
            "Install, Upgrade or Already installed per step; the reason names the module that needs it.",
          ],
          [
            "BlockerList blockers",
            "Blocker[]",
            "required",
            "Each blocker has a kind (core, conflict, missing, cycle ...) and the server's sentence.",
          ],
        ]}
      >
        <div className="stack">
          <PlanSteps plan={PLAN} nameOf={nameOf} />
          <BlockerList
            nameOf={nameOf}
            blockers={[
              {
                module: "legacy",
                kind: "conflict",
                message: "upgrading legacy to 2.0.0 would break timesheets, which needs legacy <1",
              },
              {
                module: "future",
                kind: "core",
                message: "future 1.0.0 needs core >=9, you run 0.2.0",
              },
            ]}
          />
        </div>
      </Specimen>
    </>
  );
}
