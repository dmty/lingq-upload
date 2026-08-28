type PublishedTitle = { pathname: string; title: string };

let published = $state<PublishedTitle | null>(null);

export const toolbarTitle = {
  set(pathname: string, title: string) {
    published = { pathname, title };
  },
  forPath(pathname: string) {
    return published?.pathname === pathname ? published.title : null;
  },
};
