import { expect, seed, test } from "./setup/test";
import { runFixture } from "./setup/run-fixture";

const KEY = "existing-course";

const existing = {
  id: 2962260,
  title: "Same Title",
  description: null,
  level: null,
  duration: null,
  lessons_count: 29,
  new_words_count: null,
  image_url: null,
  status: null,
  roses_count: null,
  views_count: null,
};

test.describe("first upload into a title the account already has", () => {
  test.beforeEach(async ({ page }) => {
    await seed(page, {
      ...runFixture({ key: KEY, title: "Same Title", lingqCollectionId: null }),
      __existingCourse__: existing,
    });
  });

  test("Start asks before uploading, Add pins the course then starts", async ({
    page,
  }) => {
    await page.goto(`/run/${KEY}`);
    await page.getByRole("button", { name: "Start" }).click();

    const prompt = page.getByTestId("existing-course");
    await expect(prompt).toContainText("“Same Title” with 29 lessons");
    await expect(page.getByRole("button", { name: "Start" })).toHaveCount(0);
    await expect(page.getByRole("button", { name: "Cancel" })).toHaveCount(0);

    await prompt.getByRole("button", { name: "Add to Course" }).click();

    await expect(prompt).toHaveCount(0);
    await expect(page.getByRole("button", { name: "Cancel" })).toBeVisible();
    const calls = await page.evaluate(() => window.__useCourseCalls__);
    expect(calls).toEqual([
      {
        projectId: expect.objectContaining({ content_hash: KEY }),
        collectionId: 2962260,
      },
    ]);
  });

  test("Create New starts without pinning", async ({ page }) => {
    await page.goto(`/run/${KEY}`);
    await page.getByRole("button", { name: "Start" }).click();
    await page
      .getByTestId("existing-course")
      .getByRole("button", { name: "Create New Course" })
      .click();

    await expect(page.getByTestId("existing-course")).toHaveCount(0);
    await expect(page.getByRole("button", { name: "Cancel" })).toBeVisible();
    expect(
      await page.evaluate(() => window.__useCourseCalls__),
    ).toBeUndefined();
  });

  test("a pinned project starts without asking", async ({ page }) => {
    await seed(page, runFixture({ key: KEY, title: "Same Title" }));
    await page.goto(`/run/${KEY}`);
    await page.getByRole("button", { name: "Start" }).click();

    await expect(page.getByRole("button", { name: "Cancel" })).toBeVisible();
    await expect(page.getByTestId("existing-course")).toHaveCount(0);
  });
});
