#!/usr/bin/env bash
# Builds the GitHub Pages site into target/site/out with MkDocs Material: site/home.md as the front page and docs/ beside it.
set -euo pipefail

repository_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repository_root"

blob_url="https://github.com/BeatLink/NeoTime/blob/master"
source_dir="target/site/src"

# ---------------------------------------------------------------------------
# Stage the pages
# ---------------------------------------------------------------------------

rm -rf target/site
mkdir -p "$source_dir/screens" "$source_dir/stylesheets"
cp docs/*.md "$source_dir/"
cp -r docs/images "$source_dir/images"
cp harness/snapshots/screens/*.png "$source_dir/screens/"
cp site/home.md "$source_dir/index.md"
cp site/neotime.css "$source_dir/stylesheets/neotime.css"

# The README becomes the About page, with its links into docs/ made local.
sed -e 's#](docs/#](#g' README.md > "$source_dir/introduction.md"

# Screens point into the staged copy; any other link out of docs/ goes to the file on GitHub.
for page in "$source_dir"/*.md; do
    sed -i \
        -e 's#](\.\./harness/snapshots/screens/#](screens/#g' \
        -e 's#src="\.\./harness/snapshots/screens/#src="screens/#g' \
        -e "s#](\.\./\([^)]*\))#](${blob_url}/\1)#g" \
        -e "s#](\(LICENSE\))#](${blob_url}/\1)#g" \
        "$page"
done

# Every doc must be in the navigation, so a new page cannot be left out of the site.
for page in docs/*.md; do
    name="$(basename "$page")"
    if ! grep -q ": $name\$" site/mkdocs.yml; then
        echo "site/mkdocs.yml does not list docs/$name in its nav" >&2
        exit 1
    fi
done

# ---------------------------------------------------------------------------
# Build
# ---------------------------------------------------------------------------

# Strict mode fails on a broken link or a missing image, such as a renamed snapshot on the front page.
python3 -I -m mkdocs build --strict --config-file site/mkdocs.yml

# The front page and the About page are not in docs/, so their edit buttons open the files they come from.
edit_url="https://github.com/BeatLink/NeoTime/edit/master"
sed -i "s#${edit_url}/docs/index.md#${edit_url}/site/home.md#" target/site/out/index.html
sed -i "s#${edit_url}/docs/introduction.md#${edit_url}/README.md#" target/site/out/introduction/index.html

echo "Built the site into target/site/out"
