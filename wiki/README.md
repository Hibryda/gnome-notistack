# Wiki sources

These Markdown files are the source for the project wiki. A forge wiki (GitHub or
Forgejo) is a **separate git repository** (`<repo>.wiki.git`), so publishing means
copying these pages into that repo. `Home.md` is the landing page; page links use
bare names (e.g. `[Installation](Installation)`), which both forges resolve.

## Publish to a GitHub/Forgejo wiki

Create the first wiki page through the web UI once (this initialises the wiki
repo), then:

```sh
git clone <repo-url>.wiki.git wiki-remote
cp wiki/*.md wiki-remote/
cd wiki-remote
git add -A && git commit -m "Update wiki" && git push
```

Keeping the pages here in the main repo lets them be reviewed alongside the code;
the copy step above is the only thing needed to publish them.
