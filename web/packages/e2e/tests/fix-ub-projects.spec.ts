import { expect, test } from "../support/fixtures.ts";

// A done project takes no new task (docs/ISSUES.md S10): the server refuses projects_task.create
// on it (a ref_filter on project_id), and the project panel offers no "Add a task" row there.

test("a done project takes no new task, on the server and in its panel (S10)", async ({
  page,
  signIn,
  api,
  uid,
}) => {
  const root = await api.token("root");
  const had = ((await api.run("root", "module.list")) as any[]).some((m) => m.id === "projects");
  if (!had) await api.run("root", "marketplace_listing.install", { id: "projects" });
  try {
    const project = await api.create("root", "projects_project", { name: uid("Finished job") });
    await api.create("root", "projects_task", { title: uid("Last step"), project_id: project.id });
    await api.run("root", "projects_project.start", { id: project.id });
    await api.run("root", "projects_project.finish", { id: project.id });

    const refused = await api.post(
      "/api/projects_task/create",
      { title: uid("Too late"), project_id: project.id },
      root,
    );
    expect(refused.status).toBe(400);
    expect(JSON.stringify(refused.json)).toContain("state != done");

    await signIn("root", `/projects/projects_project?id=${project.id}`);
    const panel = page.getByRole("region", { name: "Project progress" });
    await expect(panel).toBeVisible();
    await expect(panel.locator(".prj-add")).toHaveCount(0);
  } finally {
    if (!had) await api.post("/api/module/uninstall", { id: "projects" }, root);
  }
});
