# TODO & FIXME Tracking

This document tracks all identified TODO and FIXME items in the codebase, organized by priority and category for systematic resolution.

## 🔴 Critical Issues (FIXMEs)

### Memory/Resource Management

- [x] **Memory Leak - Cookie Key** (`libcub/src/impls/types/mod.rs:100`)
  - **Issue**: `FIXME: should not be static — that means we leak it`
  - **Impact**: Memory leak from static cookie key
  - **Action**: Refactor to use properly scoped key management
  - **Priority**: High

- [ ] **File Buffering** (`libcdn/src/impls/mod.rs:195`)
  - **Issue**: `TODO: don't buffer the whole file in memory`
  - **Impact**: Memory issues with large file uploads
  - **Action**: Implement streaming file upload
  - **Priority**: High

- [ ] **Hardcoded Transcode Limit** (`libmom/src/impls/deriver.rs:253`)
  - **Issue**: `FIXME: don't hardcode that limit?`
  - **Impact**: Fixed limit may not suit all systems
  - **Action**: Make `MAX_CONCURRENT_FFMPEG_TRANSCODES` configurable
  - **Priority**: Medium

### Error Handling

- [ ] **Unwrap in File Watcher** (`librevision/src/impls/watch.rs:69`)
  - **Issue**: `TODO: Do not unwrap, just mark a revision error instead`
  - **Impact**: Potential panic in file watching
  - **Action**: Replace unwrap with proper error handling
  - **Priority**: High

- [ ] **WebSocket Graceful Closing** (`libmom/src/impls/endpoints/tenant/media.rs:41`)
  - **Issue**: `FIXME: maybe implement graceful closing?`
  - **Impact**: Improper WebSocket connection termination
  - **Action**: Implement proper closing protocol per Axum docs
  - **Priority**: Medium

## 🟡 Performance & Optimization

- [ ] **Head Insert Caching** (`libcub/src/impls/cub_req/mod.rs:246`)
  - **Issue**: `TODO: a bunch of this could be cached`
  - **Impact**: Performance optimization opportunity
  - **Action**: Implement caching for head insertion logic
  - **Priority**: Medium

- [ ] **Async Task Spawning** (`librevision/src/impls/make.rs:363`)
  - **Issue**: `TODO: Spawning async tasks is not really what we want here, we want to use CPU, not do async I/O`
  - **Impact**: Inefficient use of async runtime for CPU work
  - **Action**: Use CPU-bound task executor
  - **Priority**: Medium

- [ ] **HTML Truncation Optimization** (`libhtmlrewrite/src/lib.rs:39`)
  - **Issue**: `TODO: can we do an early return here?`
  - **Impact**: Performance optimization opportunity
  - **Action**: Optimize HTML truncation logic
  - **Priority**: Low

## 🟢 Feature Implementation

### Core Features

- [x] **WebSocket Upgrade** (`libcdn/src/impls/mod.rs:27`)
  - **Issue**: `TODO: websocket upgrade`
  - **Impact**: Missing WebSocket functionality in dev mode
  - **Action**: Implement WebSocket upgrade for development server
  - **Priority**: Medium



### Content Processing

- [ ] **Crate Detection** (`librevision/src/impls/load.rs:702`)
  - **Issue**: `crates: Default::default(), // TODO`
  - **Impact**: Missing crate information in pages
  - **Action**: Implement crate detection/parsing from content
  - **Priority**: Low

- [ ] **GitHub Repo Detection** (`librevision/src/impls/load.rs:703`)
  - **Issue**: `github_repos: Default::default(), // TODO`
  - **Impact**: Missing GitHub repo information in pages
  - **Action**: Implement GitHub repo detection/parsing from content
  - **Priority**: Low

- [ ] **Rust Version Detection** (`librevision/src/impls/load.rs:722`)
  - **Issue**: `TODO: fill these in`
  - **Impact**: Missing Rust version information
  - **Action**: Extract Rust version from content/frontmatter
  - **Priority**: Low

- [ ] **Page Error Handling** (`librevision/src/impls/load.rs:671-673`)
  - **Issue**: `TODO: do something about erroring pages, better than "failing to load the revision"`
  - **Impact**: Poor error reporting for page failures
  - **Action**: Improve error reporting and recovery
  - **Priority**: Medium

### Search & Query

- [ ] **Future Content Exclusion** (`libsearch/src/lib.rs:135-137`)
  - **Issue**: `TODO: also exclude futures, but this requires looking into the syntax of tantivy's query language re: dates`
  - **Impact**: Future-dated content appears in search results
  - **Action**: Research tantivy date syntax and implement exclusion
  - **Priority**: Medium

- [ ] **Search Autocomplete Future Exclusion** (`libsearch/src/lib.rs:360-362`)
  - **Issue**: Same as above but for autocomplete
  - **Impact**: Duplicate logic and future content in autocomplete
  - **Action**: Implement and deduplicate from search_inner
  - **Priority**: Medium

- [ ] **Fallible Search Operations** (`libsearch/src/lib.rs:68`)
  - **Issue**: `TODO: fallible ops`
  - **Impact**: Missing error handling in search operations
  - **Action**: Add proper error handling throughout search module
  - **Priority**: Medium

### Caching & Storage

- [ ] **Reddit Submission Caching** (`libreddit/src/lib.rs:194`)
  - **Issue**: `TODO: cache those in the database`
  - **Impact**: Repeated API calls to Reddit
  - **Action**: Implement database caching for submissions
  - **Priority**: Low

## 🔧 Technical Debt & Refactoring

- [ ] **Serialization Format** (`librevision/src/impls/mod.rs:45`)
  - **Issue**: `TODO: switch to something else`
  - **Impact**: Using suboptimal serialization (facet_json)
  - **Action**: Evaluate alternatives (protobuf, bincode, etc.)
  - **Priority**: Low

- [ ] **Template Value Facet Support** (`libtemplate/src/global_functions_and_filters.rs:237`)
  - **Issue**: `TODO: impl facet for a wrapper of Value I guess`
  - **Impact**: Missing facet support in templates
  - **Action**: Implement proper facet wrapper for template values
  - **Priority**: Low

- [ ] **Image Types Dependency** (`libwebpage/src/lib.rs:4`)
  - **Issue**: `TODO: move me to image-types or something to avoid rebuilds`
  - **Impact**: Unnecessary rebuilds due to dependency structure
  - **Action**: Restructure dependencies to reduce build times
  - **Priority**: Low

- [ ] **Video Format Simplification** (`libmom/src/impls/deriver.rs:110`)
  - **Issue**: `TODO: it might make sense to simplify this at some point — maybe get rid of TargetFormat, not sure`
  - **Impact**: Potentially overcomplicated video transcoding architecture
  - **Action**: Review and refactor video format handling
  - **Priority**: Low

- [ ] **Facet HashSet Support** (`libmom/src/impls/endpoints.rs:80`)
  - **Issue**: `todo: add HashSet stuff to facet, for now, we work around`
  - **Impact**: Workaround for missing facet functionality
  - **Action**: Add HashSet support to facet crate or find better solution
  - **Priority**: Low

## 📋 Progress Tracking

### Summary
- **Total Items**: 21
- **Critical (High Priority)**: 4
- **Medium Priority**: 8
- **Low Priority**: 9

### By Category
- **Memory/Resource**: 3 items
- **Error Handling**: 2 items
- **Performance**: 3 items
- **Feature Implementation**: 8 items
- **Technical Debt**: 5 items

### Next Actions
1. Address critical memory leak in cookie key management
2. Fix error handling in file watcher (remove unwrap)
3. Implement file streaming for large uploads
4. Add WebSocket graceful closing
5. Research and implement future content exclusion in search

---

*Last updated: [Current Date]*
*Total items resolved: 1/21*
