/**
 * GitHub Actions Cache Cleanup Script
 *
 * Prunes `refs/heads/main`: within each cache family it keeps only the most
 * recently used entry and deletes the rest - the superseded generations that
 * a changed lock file, toolchain pin, SDK version or CodeQL release leaves
 * behind. Caches on any other ref are left alone here: an open pull request's
 * cache may be newer than main's live one and must never evict it; a closed
 * pull request's caches are reclaimed by the workflow's other job. Caches
 * matching no known family are never deleted, only reported loudly so a new
 * family cannot accumulate unnoticed.
 *
 * Families (the first segments name the family, the rest is the generation):
 *   cargo-{OS}-{hash of toolchain + lock + manifest}
 *   npm-{OS}-node-{version}-lock-{hash of package-lock.json}
 *   npm-markdownlint-{OS}-node-{version}-mdlint-{version}
 *   qt-{OS}-{version}-{kit}                      (the kit may contain a dash)
 *   vulkan-sdk-{os}-{version}-{components hash}
 *   codeql-overlay-base-database-{cache version}-{config hash}-{languages}-{cli version}-{sha}-{run id}-{attempt}
 */

module.exports = async ({ github, context, core }) => {
    const dryRun = process.env.INPUT_DRY_RUN === "true";
    const pruneRef = "refs/heads/main";
    const owner = context.repo.owner;
    const repo = context.repo.repo;

    const stats = {
        totalCaches: 0,
        deletedCaches: 0,
        freedBytes: 0,
        errors: 0,
    };

    function formatBytes(bytes) {
        if (bytes === 0) return "0 B";
        const k = 1024;
        const sizes = ["B", "KB", "MB", "GB"];
        const i = Math.floor(Math.log(bytes) / Math.log(k));
        return parseFloat((bytes / Math.pow(k, i)).toFixed(2)) + " " + sizes[i];
    }

    function getDaysSinceAccess(cache) {
        const lastAccessed = new Date(cache.last_accessed_at || cache.created_at);
        const now = new Date();
        return Math.floor((now - lastAccessed) / (1000 * 60 * 60 * 24));
    }

    function log(message, level = "info") {
        const prefix = dryRun ? "[DRY-RUN] " : "";
        if (level === "error") {
            core.error(`${prefix}${message}`);
        } else if (level === "warning") {
            core.warning(`${prefix}${message}`);
        } else {
            core.info(`${prefix}${message}`);
        }
    }

    async function listAllCaches() {
        const caches = [];
        let page = 1;

        while (true) {
            try {
                const response = await github.rest.actions.getActionsCacheList({
                    owner,
                    repo,
                    per_page: 100,
                    page,
                });

                if (response.data.actions_caches.length === 0) break;
                caches.push(...response.data.actions_caches);
                if (response.data.actions_caches.length < 100) break;
                page++;
            } catch (error) {
                if (error.status === 404) {
                    log("No caches found", "warning");
                    break;
                }
                throw error;
            }
        }

        return caches;
    }

    async function deleteCache(cache, reason) {
        if (dryRun) {
            log(`  Would delete: ${cache.key} (${formatBytes(cache.size_in_bytes)}) [${reason}]`);
            stats.freedBytes += cache.size_in_bytes;
            return true;
        }

        try {
            await github.rest.actions.deleteActionsCacheById({
                owner,
                repo,
                cache_id: cache.id,
            });
            log(`  Deleted: ${cache.key} (${formatBytes(cache.size_in_bytes)}) [${reason}]`);
            stats.freedBytes += cache.size_in_bytes;
            return true;
        } catch (error) {
            log(`  Failed to delete ${cache.key}: ${error.message}`, "error");
            stats.errors++;
            return false;
        }
    }

    // The family a key belongs to, or null for a key this script cannot name.
    // Family tests use startsWith on the whole prefix because some prefixes
    // contain dashes themselves; the more specific npm-markdownlint- test
    // comes before the general npm- one.
    function familyOf(key) {
        const parts = key.split("-");
        if (key.startsWith("cargo-")) {
            // cargo-{OS}: one generation per toolchain + lock + manifest hash.
            return parts.slice(0, 2).join("-");
        }
        if (key.startsWith("npm-markdownlint-")) {
            // npm-markdownlint-{OS}: one generation per node + markdownlint version.
            return parts.slice(0, 3).join("-");
        }
        if (key.startsWith("npm-")) {
            // npm-{OS}: one generation per node version + package-lock.json hash.
            return parts.slice(0, 2).join("-");
        }
        if (key.startsWith("qt-")) {
            // qt-{OS}-{kit}: the version segment is the generation; the kit is
            // everything after it, because llvm-mingw_64 contains a dash.
            return [parts[0], parts[1], ...parts.slice(3)].join("-");
        }
        if (key.startsWith("vulkan-")) {
            // vulkan-sdk-{os}: one generation per SDK version + components hash.
            return parts.slice(0, 3).join("-");
        }
        if (key.startsWith("codeql-overlay-base-database-")) {
            // CodeQL default setup saves one overlay-base database per push
            // to main, and the key embeds the commit SHA, run ID and attempt,
            // so no two keys ever match: grouped by full key they could never
            // be deleted and accumulated without bound. CodeQL restores via a
            // SHA-less prefix and only ever uses the newest match, so
            // everything but the newest is dead weight. Group up to and
            // including the language segment (prefix, cache version, config
            // hash, languages) so that a second analysed language or
            // configuration, should one ever appear, keeps its own newest
            // base rather than evicting this one. The CLI version is
            // deliberately excluded: keeping one cache per CodeQL release
            // would hoard a stale entry after every upgrade, whereas losing
            // the old version's base merely costs one full (slower, still
            // correct) analysis.
            return parts.slice(0, 7).join("-");
        }
        return null;
    }

    try {
        log("=".repeat(60));
        log("GitHub Actions Cache Cleanup");
        log("=".repeat(60));
        log("");
        log(`Repository:   ${owner}/${repo}`);
        log(`Pruning ref:  ${pruneRef}`);
        log(`Dry run:      ${dryRun}`);
        log("");

        const allCaches = await listAllCaches();
        stats.totalCaches = allCaches.length;

        if (allCaches.length === 0) {
            log("No caches found.");
            return;
        }

        const caches = allCaches.filter((cache) => cache.ref === pruneRef);
        const elsewhere = allCaches.length - caches.length;
        log(`Found ${allCaches.length} cache(s), ${caches.length} on ${pruneRef}.`);
        if (elsewhere > 0) {
            // A pull request's caches are its own until it closes; the reclaim
            // job deletes them then. Listed so a leak on some other ref is seen.
            const otherRefs = [...new Set(allCaches.filter((c) => c.ref !== pruneRef).map((c) => c.ref))];
            log(`${elsewhere} cache(s) on other refs are left alone here: ${otherRefs.join(", ")}`);
        }
        log("");

        const cacheGroups = {};
        const unknownKeys = [];
        for (const cache of caches) {
            let prefix = familyOf(cache.key);
            if (prefix === null) {
                // Deliberate fail-safe: never guess at a family this script
                // cannot name, because a wrong grouping could delete a live,
                // reused cache. The full key becomes the group name, so the
                // entry is never deleted; the warning below keeps the pile-up
                // visible instead of silent.
                prefix = cache.key;
                unknownKeys.push(cache.key);
            }

            if (!cacheGroups[prefix]) {
                cacheGroups[prefix] = [];
            }
            cacheGroups[prefix].push(cache);
        }

        if (unknownKeys.length > 0) {
            // Silent singleton groups are how 15 CodeQL caches went unnoticed;
            // a warning annotation makes any future unknown family loud. One
            // aggregated annotation rather than one per key, because GitHub
            // caps annotations per step and a large pile-up would be truncated.
            log(`${unknownKeys.length} cache(s) match no known family and will never be deleted by this script:\n  ${unknownKeys.join("\n  ")}`, "warning");
        }

        for (const [prefix, groupCaches] of Object.entries(cacheGroups)) {
            // Sort by last_accessed_at descending (most recent first), falling
            // back to created_at so a never-restored cache still orders sanely.
            groupCaches.sort((a, b) =>
                new Date(b.last_accessed_at || b.created_at) - new Date(a.last_accessed_at || a.created_at));

            log(`Group: ${prefix} (${groupCaches.length} cache(s))`);

            // Keep the first (most recent), delete the rest
            if (groupCaches.length > 0) {
                log(`  Keeping: ${groupCaches[0].key}`);
            }

            for (let i = 1; i < groupCaches.length; i++) {
                const cache = groupCaches[i];
                const daysSinceAccess = getDaysSinceAccess(cache);
                if (await deleteCache(cache, `superseded, ${daysSinceAccess}d since last access`)) {
                    stats.deletedCaches++;
                }
            }
        }

        log("");
        log("=".repeat(60));
        log("Summary");
        log("=".repeat(60));
        log(`  Total caches:  ${stats.totalCaches}`);
        log(`  On ${pruneRef}: ${caches.length}`);
        log(`  Deleted:       ${stats.deletedCaches}`);
        log(`  Kept:          ${stats.totalCaches - stats.deletedCaches}`);
        log(`  Space freed:   ${formatBytes(stats.freedBytes)}`);

        if (stats.errors > 0) {
            log(`  Errors:        ${stats.errors}`, "warning");
        }

        if (dryRun) {
            log("");
            log("Dry run - no caches were actually deleted.", "warning");
        }

        core.setOutput("deleted_count", stats.deletedCaches);
        core.setOutput("freed_bytes", stats.freedBytes);

        log("");
        log("Cleanup completed");
    } catch (error) {
        core.setFailed(`Cleanup failed: ${error.message}`);
        throw error;
    }
};
