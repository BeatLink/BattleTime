#!/usr/bin/env bash
# Builds the GitHub Pages site into battletime/target/site/out with MkDocs Material: battletime/site/home.md as the front page and docs/ beside it.
set -euo pipefail

# The site's sources and build output sit in battletime/; the docs and the README at the top of the repository.
repository_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repository_root/battletime"

# Links climbing out of docs/ land at the top of the repository.
blob_url="https://github.com/BeatLink/BattleTime/blob/master"
source_dir="target/site/src"

# ---------------------------------------------------------------------------
# Stage the pages
# ---------------------------------------------------------------------------

rm -rf target/site
mkdir -p "$source_dir/screens" "$source_dir/stylesheets"
cp -r ../docs/. "$source_dir/"
cp harness/snapshots/screens/*.png "$source_dir/screens/"
cp site/home.md "$source_dir/index.md"
cp site/battletime.css "$source_dir/stylesheets/battletime.css"

# The repository's README becomes the About page, with its links into docs/ made local and the rest sent to GitHub.
sed -e "s#](docs/)#](https://github.com/BeatLink/BattleTime/tree/master/docs/)#g" \
    -e "s#](\(tools/[^)]*\|battletime/[^)]*\|companion/[^)]*\))#](https://github.com/BeatLink/BattleTime/tree/master/\1)#g" \
    -e 's#](docs/#](#g' \
    ../README.md > "$source_dir/introduction.md"

# Screens point into the staged copy; any other link out of docs/ goes to the file on GitHub.
for page in "$source_dir"/*/*.md; do
    sed -i \
        -e 's#](\.\./\.\./battletime/harness/snapshots/screens/#](../screens/#g' \
        -e 's#src="\.\./\.\./battletime/harness/snapshots/screens/#src="../screens/#g' \
        -e "s#](\.\./\.\./\([^)]*\))#](${blob_url}/\1)#g" \
        "$page"
done
for page in "$source_dir"/*.md; do
    sed -i \
        -e 's#](\.\./battletime/harness/snapshots/screens/#](screens/#g' \
        -e 's#src="\.\./battletime/harness/snapshots/screens/#src="screens/#g' \
        -e "s#](\.\./\([^)]*\))#](${blob_url}/\1)#g" \
        -e "s#](\(LICENSE\))#](${blob_url}/\1)#g" \
        "$page"
done

# Every doc must be in the navigation, so a new page cannot be left out of the site.
for page in ../docs/*.md ../docs/*/*.md; do
    name="${page#../docs/}"
    [[ "$name" == apps/* ]] && continue
    if ! grep -q ": $name\$" site/mkdocs.yml; then
        echo "site/mkdocs.yml does not list docs/$name in its nav" >&2
        exit 1
    fi
done

# Each app's page joins the Apps tab, in order of its title, under the line that marks their place.
app_entries="$(for page in ../docs/apps/*.md; do
    printf '      - "%s": apps/%s\n' "$(sed -n 's/^# //p;q' "$page")" "$(basename "$page")"
done | sort)"
config="target/site/mkdocs.yml"
while IFS= read -r line; do
    printf '%s\n' "$line"
    if [[ "$line" == *"lists a page for each app"* ]]; then
        printf '%s\n' "$app_entries"
    fi
done < site/mkdocs.yml | sed -e 's#\.\./target/site/##' > "$config"

# ---------------------------------------------------------------------------
# Build
# ---------------------------------------------------------------------------

# Strict mode fails on a broken link or a missing image, such as a renamed snapshot on the front page.
python3 -I -m mkdocs build --strict --config-file "$config"

# The front page and the About page are not in docs/, so their edit buttons open the files they come from.
edit_url="https://github.com/BeatLink/BattleTime/edit/master"
sed -i "s#${edit_url}/docs/index.md#${edit_url}/battletime/site/home.md#" target/site/out/index.html
sed -i "s#${edit_url}/docs/introduction.md#${edit_url}/README.md#" target/site/out/introduction/index.html

echo "Built the site into battletime/target/site/out"
