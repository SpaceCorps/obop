import { expect, test } from "../support/fixtures.ts";

// Projects (wave 2, services, plan 3.5): a project opens on its progress, its milestones in date
// order and its open tasks as a checklist. Ticking a task runs the action that moves it to done
// from where it is (complete_now from todo, complete from doing); "Add a task" creates one in the
// project. Both are commands the API can run too.

test("a project shows its progress, ticks a task done and adds one", async ({
  page,
  signIn,
  api,
  uid,
}) => {
  const root = await api.token("root");
  await api.post("/api/marketplace_listing/install", { id: "projects" }, root);
  try {
    const project = await api.create("root", "projects_project", { name: uid("Shop relaunch") });
    const stone = await api.create("root", "projects_milestone", {
      project_id: project.id,
      name: uid("Go-live"),
      due_on: "2026-12-15",
    });
    const todo = await api.create("root", "projects_task", {
      title: uid("Write texts"),
      project_id: project.id,
    });
    const doing = await api.create("root", "projects_task", {
      title: uid("Test checkout"),
      project_id: project.id,
    });
    await api.run("root", "projects_task.begin", { id: doing.id });

    await signIn("root", `/projects/projects_project?id=${project.id}`);
    const panel = page.getByRole("region", { name: "Project progress" });
    await expect(panel).toContainText("0 of 2 tasks done");
    await expect(panel).toContainText(stone.name);

    // A todo task is ticked straight to done.
    await panel.getByRole("checkbox", { name: `Mark ${todo.title} as done` }).click();
    await expect(panel).toContainText("1 of 2 tasks done");
    expect((await api.run("root", "projects_task.get", { id: todo.id })).state).toBe("done");
    await expect(panel.getByRole("checkbox", { name: `Mark ${todo.title} as done` })).toHaveCount(
      0,
    );
    await expect(panel).toContainText("50%");

    // A new task lands in the project.
    const title = uid("Announce the shop");
    await panel.getByLabel("New task title").fill(title);
    await panel.getByLabel("New task title").press("Enter");
    await expect(panel.getByRole("link", { name: title })).toBeVisible();
    const made = (await api.list("root", "projects_task")).find((r) => r.title === title);
    expect(made?.project_id).toBe(project.id);
    expect(made?.state).toBe("todo");
    await expect(panel).toContainText("1 of 3 tasks done");
  } finally {
    await api.post("/api/module/uninstall", { id: "projects" }, root);
  }
});
