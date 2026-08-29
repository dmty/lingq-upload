import { expect, seed, test } from "./setup/test";
import { libraryEntry } from "./setup/library-fixture";

const KEY = "library-cover";
const IMAGE_URL = "https://cdn.lingq.com/covers/toki.webp";

test("library shows the LingQ cover when the project has no local file", async ({
  page,
}) => {
  const png = Buffer.from(
    "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==",
    "base64",
  );
  await page.route("https://cdn.lingq.com/**", (route) =>
    route.fulfill({ status: 200, contentType: "image/png", body: png }),
  );

  await seed(page, {
    __libraryEntries__: [
      libraryEntry(KEY, {
        title: "時をかける少女",
        language: "ja",
        completed_lesson_count: 6,
        receipt_count: 6,
        lingq_collection_id: 2792683,
        cover_path: null,
        status: "done",
      }),
    ],
    __courseView__: {
      collection: {
        id: 2792683,
        title: "時をかける少女",
        description: null,
        level: null,
        duration: null,
        lessons_count: 6,
        new_words_count: null,
        image_url: IMAGE_URL,
        status: "private",
        roses_count: null,
        views_count: null,
      },
      lessons: [],
    },
  });
  await page.goto("/library");

  const img = page.getByRole("listbox", { name: "Library" }).locator("img");
  await expect(img).toHaveAttribute("src", IMAGE_URL);
});
