let dismissed = $state(false);

export const libraryBanner = {
  get dismissed() {
    return dismissed;
  },
  set dismissed(v: boolean) {
    dismissed = v;
  },
};
