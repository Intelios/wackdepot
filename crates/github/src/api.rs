use std::collections::HashMap;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicU64, Ordering},
};

use crate::{
    auth::{self, OAuthConfig, TokenStore, Tokens},
    http::{self, Request, Response, Transport},
    types::*,
};
use serde::de::DeserializeOwned;
use serde_json::{Value, json};

#[derive(Clone)]
pub struct Client {
    transport: Arc<dyn Transport>,
    store: Arc<dyn TokenStore>,
    config: Option<OAuthConfig>,
    tokens: Arc<Mutex<Option<Tokens>>>,
    cache: Arc<Mutex<HashMap<String, (String, Vec<u8>)>>>,
    blocked_until: Arc<AtomicU64>,
}

impl Client {
    pub fn new() -> Self {
        Self::with_transport(
            Arc::new(http::HttpTransport::default()),
            Arc::new(auth::Keychain),
            OAuthConfig::configured().ok(),
        )
    }
    pub fn with_transport(
        transport: Arc<dyn Transport>,
        store: Arc<dyn TokenStore>,
        config: Option<OAuthConfig>,
    ) -> Self {
        Self {
            transport,
            store,
            config,
            tokens: Arc::new(Mutex::new(None)),
            cache: Arc::default(),
            blocked_until: Arc::default(),
        }
    }
    pub fn transport(&self) -> &dyn Transport {
        self.transport.as_ref()
    }
    pub fn connected(&self) -> Result<bool, String> {
        Ok(self.store.load()?.is_some())
    }
    pub fn install_tokens(&self, tokens: Tokens) -> Result<User, String> {
        self.store.save(&tokens)?;
        *self
            .tokens
            .lock()
            .map_err(|_| "Couldn't update the GitHub session.".to_string())? = Some(tokens);
        self.invalidate();
        self.viewer()
    }
    pub fn disconnect(&self) -> Result<(), String> {
        self.store.clear()?;
        *self
            .tokens
            .lock()
            .map_err(|_| "Couldn't close the GitHub session.".to_string())? = None;
        self.invalidate();
        Ok(())
    }
    pub fn invalidate(&self) {
        if let Ok(mut cache) = self.cache.lock() {
            cache.clear();
        }
    }
    pub fn retry_at(&self) -> u64 {
        self.blocked_until.load(Ordering::SeqCst)
    }
    fn token(&self, force: bool) -> Result<String, String> {
        let mut guard = self
            .tokens
            .lock()
            .map_err(|_| "Couldn't access the GitHub session.".to_string())?;
        // A Git credential helper may have refreshed the Keychain session.
        let mut tokens = self
            .store
            .load()?
            .ok_or("Connect GitHub in Settings to use pull requests.")?;
        if force
            || tokens
                .expires_at
                .is_some_and(|expiry| expiry <= auth::now() + 60)
        {
            tokens=auth::refresh(self.config.as_ref().ok_or("This build needs its GitHub OAuth client configuration before the session can be refreshed.")?,&tokens,self.transport.as_ref())?;
            self.store.save(&tokens)?;
        }
        let token = tokens.access_token.clone();
        *guard = Some(tokens);
        Ok(token)
    }
    fn request(
        &self,
        method: &'static str,
        path: &str,
        body: Option<Value>,
    ) -> Result<Value, String> {
        if self.retry_at() > auth::now() {
            return Err(
                "GitHub has temporarily limited requests. Wait a moment before refreshing again."
                    .into(),
            );
        }
        if !path.starts_with('/') || path.starts_with("//") {
            return Err("That GitHub API path isn't valid.".into());
        }
        let url = format!("https://api.github.com{path}");
        let cached = if method == "GET" {
            self.cache.lock().ok().and_then(|c| c.get(&url).cloned())
        } else {
            None
        };
        let make_request = |token: String| {
            let mut headers = vec![
                ("Accept".into(), "application/vnd.github+json".into()),
                ("User-Agent".into(), "WackDepot/0.1".into()),
                ("X-GitHub-Api-Version".into(), http::API_VERSION.into()),
                ("Authorization".into(), format!("Bearer {token}")),
            ];
            if body.is_some() {
                headers.push(("Content-Type".into(), "application/json".into()));
            }
            if let Some((etag, _)) = &cached {
                headers.push(("If-None-Match".into(), etag.clone()));
            }
            Request {
                method,
                url: url.clone(),
                headers,
                body: body.as_ref().map(|b| b.to_string().into_bytes()),
            }
        };
        let mut response=self.transport.send(make_request(self.token(false)?)).map_err(|error|if method=="GET" || path=="/graphql" && body.as_ref().and_then(|b|b["query"].as_str()).is_some_and(|q|q.trim_start().starts_with("query")) {error} else {"GitHub didn't confirm whether this action completed. Refresh its details before trying again.".into()})?;
        if response.status == 401
            && self
                .store
                .load()?
                .is_some_and(|t| t.refresh_token.is_some())
        {
            response = self.transport.send(make_request(self.token(true)?))?;
        }
        self.rate_limit(&response);
        if response.status == 304 {
            return decode(
                &cached
                    .ok_or("GitHub returned an expired cached response. Refresh again.")?
                    .1,
            );
        }
        if !(200..300).contains(&response.status) {
            return Err(http::error(&response));
        }
        if method == "GET" {
            if let Some(etag) = response.headers.get("etag")
                && let Ok(mut cache) = self.cache.lock()
            {
                if cache.len() >= 64 {
                    cache.clear();
                }
                cache.insert(url, (etag.clone(), response.body.clone()));
            }
        } else if path != "/graphql"
            || body
                .as_ref()
                .and_then(|b| b["query"].as_str())
                .is_some_and(|q| q.trim_start().starts_with("mutation"))
        {
            self.invalidate();
        }
        if response.body.is_empty() {
            Ok(Value::Null)
        } else {
            decode(&response.body)
        }
    }
    fn rate_limit(&self, response: &Response) {
        let now = auth::now();
        let retry = response
            .headers
            .get("retry-after")
            .and_then(|v| v.parse::<u64>().ok())
            .map(|s| now + s);
        let reset = response
            .headers
            .get("x-ratelimit-reset")
            .and_then(|v| v.parse::<u64>().ok());
        if response.status == 429
            || response
                .headers
                .get("x-ratelimit-remaining")
                .is_some_and(|v| v == "0")
            || response.status == 403 && retry.is_some()
        {
            self.blocked_until.store(
                retry.or(reset).unwrap_or(now + 60).max(now + 1),
                Ordering::SeqCst,
            );
        }
    }
    pub fn graphql(&self, query: &str, variables: Value) -> Result<Value, String> {
        let value = self.request(
            "POST",
            "/graphql",
            Some(json!({"query":query,"variables":variables})),
        )?;
        if let Some(errors) = value["errors"].as_array()
            && !errors.is_empty()
        {
            return Err(format!(
                "GitHub couldn't complete this request. {}",
                errors[0]["message"]
                    .as_str()
                    .unwrap_or("Refresh and try again.")
            ));
        }
        Ok(value["data"].clone())
    }
    fn paged<T: DeserializeOwned>(
        &self,
        path: &str,
        field: Option<&str>,
        cap: usize,
    ) -> Result<(Vec<T>, bool), String> {
        let mut items = Vec::new();
        let mut page = 1;
        loop {
            let sep = if path.contains('?') { '&' } else { '?' };
            let value =
                self.request("GET", &format!("{path}{sep}per_page=100&page={page}"), None)?;
            let value = field.map_or(&value, |field| &value[field]);
            let batch: Vec<T> = serde_json::from_value(value.clone()).map_err(|_| {
                "GitHub returned an unreadable list. Refresh to try again.".to_string()
            })?;
            let count = batch.len();
            items.extend(batch);
            if count < 100 {
                return Ok((items, true));
            }
            if items.len() >= cap {
                return Ok((items, false));
            }
            page += 1;
        }
    }
    pub fn viewer(&self) -> Result<User, String> {
        from_value(self.request("GET", "/user", None)?)
    }
    pub fn repository(&self, id: &RepositoryId) -> Result<Repository, String> {
        from_value(self.request("GET", &format!("/repos/{}", id.full_name()), None)?)
    }
    pub fn pull(&self, id: &RepositoryId, number: u64) -> Result<PullRequest, String> {
        from_value(self.request(
            "GET",
            &format!("/repos/{}/pulls/{number}", id.full_name()),
            None,
        )?)
    }

    pub fn list(
        &self,
        id: &RepositoryId,
        filter: Filter,
        text: &str,
        login: &str,
        cursor: Option<&str>,
    ) -> Result<PullRequestPage, String> {
        let text = text.trim();
        // Treat a number as a direct lookup; search indexing need not have
        // caught up with a just-created pull request.
        if let Ok(number) = text.trim_start_matches('#').parse::<u64>() {
            return self.pull(id, number).map(|pull| PullRequestPage {
                items: vec![pull],
                ..Default::default()
            });
        }
        let query = format!(
            "repo:{} is:pr {} {} sort:updated-desc",
            id.full_name(),
            filter.query(login),
            text.replace(['\r', '\n'], " ")
        );
        let data=self.graphql("query PullRequests($query:String!,$after:String){search(query:$query,type:ISSUE,first:30,after:$after){issueCount pageInfo{hasNextPage endCursor} nodes{... on PullRequest{id number title body state isDraft mergedAt url author{login} headRefName headRefOid headRepository{databaseId name owner{login}} baseRefName baseRefOid repository{databaseId name owner{login}} labels(first:100){nodes{name}}}}}}",json!({"query":query,"after":cursor}))?;
        let search = &data["search"];
        let nodes = search["nodes"]
            .as_array()
            .ok_or("GitHub returned an unreadable pull request list.")?;
        let items = nodes
            .iter()
            .filter(|v| v["number"].is_u64())
            .map(summary)
            .collect();
        Ok(PullRequestPage {
            items,
            has_next: search["pageInfo"]["hasNextPage"].as_bool().unwrap_or(false),
            next_cursor: search["pageInfo"]["endCursor"].as_str().map(str::to_string),
            incomplete: search["issueCount"].as_u64().unwrap_or(0) > 1000,
        })
    }

    pub fn detail(
        &self,
        id: &RepositoryId,
        number: u64,
        login: &str,
    ) -> Result<PullRequestDetail, String> {
        let pull = self.pull(id, number)?;
        let root = format!("/repos/{}/pulls/{number}", id.full_name());
        let (files, all_files) = self.paged::<ChangedFile>(&format!("{root}/files"), None, 3000)?;
        let (commits, _) = self.paged(&format!("{root}/commits"), None, 10000)?;
        let (comments, _) = self.paged(
            &format!("/repos/{}/issues/{number}/comments", id.full_name()),
            None,
            10000,
        )?;
        let reviews = self.review_info(&pull.node_id, login)?;
        let checks = self.checks(id, &pull.head.sha)?;
        let latest = self.pull(id, number)?;
        if latest.head.sha != pull.head.sha || latest.base.sha != pull.base.sha {
            return Err(
                "This pull request changed while loading. Refresh to view a consistent revision."
                    .into(),
            );
        }
        let files_complete = all_files || files.len() as u64 == pull.changed_files;
        Ok(PullRequestDetail {
            pull: latest,
            files,
            files_complete,
            commits,
            comments,
            reviews,
            checks,
        })
    }

    pub fn review_info(&self, node: &str, login: &str) -> Result<ReviewInfo, String> {
        let mut info = ReviewInfo::default();
        let mut after = None::<String>;
        loop {
            let data=self.graphql("query ReviewThreads($id:ID!,$after:String){node(id:$id){... on PullRequest{reviewDecision mergeStateStatus viewerCanUpdate reviews(last:100,states:PENDING){nodes{id body author{login} commit{oid}}} reviewThreads(first:50,after:$after){pageInfo{hasNextPage endCursor} nodes{id path line startLine diffSide isResolved isOutdated viewerCanResolve viewerCanUnresolve comments(first:100){nodes{id body author{login} createdAt viewerCanUpdate viewerCanDelete diffHunk pullRequestReview{state}} pageInfo{hasNextPage endCursor}}}}}}}",json!({"id":node,"after":after}))?;
            let pull = &data["node"];
            if pull.is_null() {
                return Err("That pull request is no longer available to this account.".into());
            }
            info.decision = string(pull, "reviewDecision");
            info.merge_state = string(pull, "mergeStateStatus");
            info.can_update = pull["viewerCanUpdate"].as_bool().unwrap_or(false);
            if let Some(reviews) = pull["reviews"]["nodes"].as_array() {
                for review in reviews {
                    if review["author"]["login"]
                        .as_str()
                        .is_some_and(|s| s.eq_ignore_ascii_case(login))
                    {
                        info.pending_id = review["id"].as_str().map(str::to_string);
                        info.pending_head = review["commit"]["oid"].as_str().map(str::to_string);
                        info.pending_body = string(review, "body");
                    }
                }
            }
            if let Some(threads) = pull["reviewThreads"]["nodes"].as_array() {
                for raw in threads {
                    let mut comments = raw["comments"]["nodes"]
                        .as_array()
                        .cloned()
                        .unwrap_or_default();
                    let mut page = raw["comments"]["pageInfo"].clone();
                    while page["hasNextPage"].as_bool() == Some(true) {
                        let data=self.graphql("query ThreadComments($id:ID!,$after:String){node(id:$id){... on PullRequestReviewThread{comments(first:100,after:$after){nodes{id body author{login} createdAt viewerCanUpdate viewerCanDelete diffHunk pullRequestReview{state}} pageInfo{hasNextPage endCursor}}}}}",json!({"id":raw["id"],"after":page["endCursor"]}))?;
                        comments.extend(
                            data["node"]["comments"]["nodes"]
                                .as_array()
                                .cloned()
                                .unwrap_or_default(),
                        );
                        page = data["node"]["comments"]["pageInfo"].clone();
                    }
                    info.threads.push(ReviewThread {
                        id: string(raw, "id"),
                        path: string(raw, "path"),
                        line: raw["line"].as_u64().map(|n| n as u32),
                        start_line: raw["startLine"].as_u64().map(|n| n as u32),
                        side: if raw["diffSide"] == "LEFT" {
                            DiffSide::Left
                        } else {
                            DiffSide::Right
                        },
                        resolved: raw["isResolved"].as_bool().unwrap_or(false),
                        outdated: raw["isOutdated"].as_bool().unwrap_or(false),
                        can_resolve: raw["viewerCanResolve"].as_bool().unwrap_or(false),
                        can_unresolve: raw["viewerCanUnresolve"].as_bool().unwrap_or(false),
                        comments: comments
                            .iter()
                            .map(|c| ReviewComment {
                                id: string(c, "id"),
                                body: string(c, "body"),
                                author: string(&c["author"], "login"),
                                created_at: string(c, "createdAt"),
                                can_update: c["viewerCanUpdate"].as_bool().unwrap_or(false),
                                can_delete: c["viewerCanDelete"].as_bool().unwrap_or(false),
                                pending: c["pullRequestReview"]["state"] == "PENDING",
                                diff_hunk: string(c, "diffHunk"),
                            })
                            .collect(),
                    });
                }
            }
            if pull["reviewThreads"]["pageInfo"]["hasNextPage"].as_bool() != Some(true) {
                break;
            }
            after = pull["reviewThreads"]["pageInfo"]["endCursor"]
                .as_str()
                .map(str::to_string);
            if after.is_none() {
                return Err("GitHub couldn't load the next page of review threads.".into());
            }
        }
        let mut after = None::<String>;
        loop {
            let data=self.graphql("query ViewedFiles($id:ID!,$after:String){node(id:$id){... on PullRequest{files(first:100,after:$after){nodes{path viewerViewedState} pageInfo{hasNextPage endCursor}}}}}",json!({"id":node,"after":after}))?;
            let files = &data["node"]["files"];
            for file in files["nodes"].as_array().into_iter().flatten() {
                if file["viewerViewedState"] == "VIEWED" {
                    info.viewed.push(string(file, "path"));
                }
            }
            if files["pageInfo"]["hasNextPage"].as_bool() != Some(true) {
                break;
            }
            after = files["pageInfo"]["endCursor"].as_str().map(str::to_string);
            if after.is_none() {
                break;
            }
        }
        Ok(info)
    }

    pub fn checks(&self, id: &RepositoryId, sha: &str) -> Result<Checks, String> {
        let root = format!("/repos/{}", id.full_name());
        let (runs, _) = self.paged::<Value>(
            &format!("{root}/commits/{sha}/check-runs?filter=latest"),
            Some("check_runs"),
            10000,
        )?;
        let (statuses, _) =
            self.paged::<Value>(&format!("{root}/commits/{sha}/statuses"), None, 10000)?;
        let mut checks = Vec::new();
        for run in runs {
            let detail = run["conclusion"]
                .as_str()
                .unwrap_or_else(|| run["status"].as_str().unwrap_or("pending"))
                .to_string();
            checks.push(Check {
                name: string(&run, "name"),
                state: check_state(&detail),
                detail,
                url: string(&run, "details_url"),
            });
        }
        let mut seen = std::collections::HashSet::new();
        for status in statuses {
            let name = string(&status, "context");
            if seen.insert(name.clone()) {
                let detail = string(&status, "state");
                checks.push(Check {
                    name,
                    state: check_state(&detail),
                    detail,
                    url: string(&status, "target_url"),
                });
            }
        }
        // Actions permissions may be narrower than check visibility.
        let workflows = self
            .paged::<WorkflowRun>(
                &format!("{root}/actions/runs?head_sha={sha}"),
                Some("workflow_runs"),
                1000,
            )
            .map(|v| v.0)
            .unwrap_or_default();
        Ok(Checks {
            checks,
            workflows,
            jobs: Vec::new(),
        })
    }
    pub fn jobs(&self, id: &RepositoryId, run: u64) -> Result<Vec<WorkflowJob>, String> {
        self.paged(
            &format!(
                "/repos/{}/actions/runs/{run}/jobs?filter=latest",
                id.full_name()
            ),
            Some("jobs"),
            10000,
        )
        .map(|v| v.0)
    }
    pub fn rerun(&self, id: &RepositoryId, run: u64, failed: bool) -> Result<(), String> {
        self.request(
            "POST",
            &format!(
                "/repos/{}/actions/runs/{run}/{}",
                id.full_name(),
                if failed { "rerun-failed-jobs" } else { "rerun" }
            ),
            Some(json!({})),
        )
        .map(|_| ())
    }
    pub fn rerun_job(&self, id: &RepositoryId, job: u64) -> Result<(), String> {
        self.request(
            "POST",
            &format!("/repos/{}/actions/jobs/{job}/rerun", id.full_name()),
            Some(json!({})),
        )
        .map(|_| ())
    }

    pub fn compare(&self, id: &RepositoryId, base: &str, head: &str) -> Result<Comparison, String> {
        let path = format!(
            "/repos/{}/compare/{}...{}",
            id.full_name(),
            encode(base),
            encode(head)
        );
        let first = self.request("GET", &format!("{path}?per_page=100"), None)?;
        let files: Vec<ChangedFile> = from_value(first["files"].clone())?;
        let mut commits: Vec<PullCommit> = from_value(first["commits"].clone())?;
        let total = first["total_commits"].as_u64().unwrap_or(0) as usize;
        let mut page = 2;
        while commits.len() < total {
            let data = self.request("GET", &format!("{path}?per_page=100&page={page}"), None)?;
            let next: Vec<PullCommit> = from_value(data["commits"].clone())?;
            if next.is_empty() {
                break;
            }
            commits.extend(next);
            page += 1;
        }
        Ok(Comparison {
            complete: files.len() < 300 && commits.len() == total,
            files,
            head_sha: first["commits"]
                .as_array()
                .and_then(|cs| cs.last())
                .and_then(|c| c["sha"].as_str())
                .unwrap_or_default()
                .into(),
            commits,
        })
    }
    pub fn create(
        &self,
        id: &RepositoryId,
        input: &CreatePullRequestInput,
    ) -> Result<CreateOutcome, String> {
        if input.title.trim().is_empty()
            || input.base.trim().is_empty()
            || input.head.trim().is_empty()
        {
            return Err(
                "Give the pull request a title and choose its base and head branches.".into(),
            );
        }
        // Reconcile a previous create whose response may have been lost.
        let existing: Vec<PullRequest> = from_value(self.request(
            "GET",
            &format!(
                "/repos/{}/pulls?state=open&head={}&base={}",
                id.full_name(),
                encode(&input.head),
                encode(&input.base)
            ),
            None,
        )?)?;
        let pull = if let Some(pull) = existing.into_iter().next() {
            pull
        } else {
            from_value(self.request("POST",&format!("/repos/{}/pulls",id.full_name()),Some(json!({"title":input.title.trim(),"body":input.body,"base":input.base,"head":input.head,"head_repo":input.head_repo,"draft":input.draft})))?)?
        };
        let metadata_error = self.assign_metadata(id, pull.number, input).err();
        Ok(CreateOutcome {
            pull,
            metadata_error,
        })
    }
    pub fn assign_metadata(
        &self,
        id: &RepositoryId,
        number: u64,
        input: &CreatePullRequestInput,
    ) -> Result<(), String> {
        if !input.labels.is_empty() {
            self.request(
                "POST",
                &format!("/repos/{}/issues/{number}/labels", id.full_name()),
                Some(json!({"labels":input.labels})),
            )?;
        }
        if !input.reviewers.is_empty() || !input.teams.is_empty() {
            self.request(
                "POST",
                &format!(
                    "/repos/{}/pulls/{number}/requested_reviewers",
                    id.full_name()
                ),
                Some(json!({"reviewers":input.reviewers,"team_reviewers":input.teams})),
            )?;
        }
        Ok(())
    }
    pub fn edit(
        &self,
        id: &RepositoryId,
        number: u64,
        title: &str,
        body: &str,
    ) -> Result<(), String> {
        self.request(
            "PATCH",
            &format!("/repos/{}/pulls/{number}", id.full_name()),
            Some(json!({"title":title,"body":body})),
        )
        .map(|_| ())
    }
    pub fn state(&self, id: &RepositoryId, number: u64, open: bool) -> Result<(), String> {
        self.request(
            "PATCH",
            &format!("/repos/{}/pulls/{number}", id.full_name()),
            Some(json!({"state":if open {"open"}else{"closed"}})),
        )
        .map(|_| ())
    }
    pub fn set_draft(&self, node: &str, draft: bool) -> Result<(), String> {
        let query = if draft {
            "mutation Draft($input:ConvertPullRequestToDraftInput!){convertPullRequestToDraft(input:$input){pullRequest{id}}}"
        } else {
            "mutation Ready($input:MarkPullRequestReadyForReviewInput!){markPullRequestReadyForReview(input:$input){pullRequest{id}}}"
        };
        self.graphql(query, json!({"input":{"pullRequestId":node}}))
            .map(|_| ())
    }
    pub fn comment(&self, id: &RepositoryId, number: u64, body: &str) -> Result<(), String> {
        if body.trim().is_empty() {
            return Err("Write a comment before posting it.".into());
        }
        self.request(
            "POST",
            &format!("/repos/{}/issues/{number}/comments", id.full_name()),
            Some(json!({"body":body})),
        )
        .map(|_| ())
    }
    pub fn comment_line(
        &self,
        id: &RepositoryId,
        number: u64,
        anchor: &ReviewAnchor,
        body: &str,
    ) -> Result<(), String> {
        anchor.validate(&self.pull(id, number)?.head.sha)?;
        if body.trim().is_empty() {
            return Err("Write a comment before posting it.".into());
        }
        let mut input = json!({"body":body,"commit_id":anchor.head_sha,"path":anchor.path,"line":anchor.line,"side":anchor.side.api()});
        if let Some(start) = anchor.start_line {
            input["start_line"] = json!(start);
            input["start_side"] = json!(anchor.side.api());
        }
        self.request(
            "POST",
            &format!("/repos/{}/pulls/{number}/comments", id.full_name()),
            Some(input),
        )
        .map(|_| ())
    }
    pub fn branch_pull(
        &self,
        id: &RepositoryId,
        owner: &str,
        branch: &str,
    ) -> Result<Option<PullRequest>, String> {
        let pulls: Vec<PullRequest> = from_value(self.request(
            "GET",
            &format!(
                "/repos/{}/pulls?state=open&head={}",
                id.full_name(),
                encode(&format!("{owner}:{branch}"))
            ),
            None,
        )?)?;
        Ok(pulls.into_iter().find(|p| p.head.name == branch))
    }
    pub fn begin_review(&self, node: &str, sha: &str) -> Result<String, String> {
        let data=self.graphql("mutation BeginReview($input:AddPullRequestReviewInput!){addPullRequestReview(input:$input){pullRequestReview{id}}}",json!({"input":{"pullRequestId":node,"commitOID":sha}}))?;
        data["addPullRequestReview"]["pullRequestReview"]["id"]
            .as_str()
            .map(str::to_string)
            .ok_or_else(|| {
                "GitHub didn't return the pending review. Refresh before adding another comment."
                    .into()
            })
    }
    pub fn add_thread(
        &self,
        node: &str,
        review: Option<&str>,
        anchor: &ReviewAnchor,
        head: &str,
        body: &str,
    ) -> Result<(), String> {
        anchor.validate(head)?;
        if body.trim().is_empty() {
            return Err("Write a comment before adding it to the review.".into());
        }
        let mut input = json!({"pullRequestId":node,"body":body,"path":anchor.path,"line":anchor.line,"side":anchor.side.api()});
        if let Some(review) = review {
            input["pullRequestReviewId"] = json!(review);
        }
        if let Some(start) = anchor.start_line {
            input["startLine"] = json!(start);
            input["startSide"] = json!(anchor.side.api());
        }
        self.graphql("mutation ReviewThread($input:AddPullRequestReviewThreadInput!){addPullRequestReviewThread(input:$input){thread{id}}}",json!({"input":input})).map(|_|())
    }
    pub fn reply(&self, thread: &str, body: &str) -> Result<(), String> {
        if body.trim().is_empty() {
            return Err("Write a reply before posting it.".into());
        }
        self.graphql("mutation Reply($input:AddPullRequestReviewThreadReplyInput!){addPullRequestReviewThreadReply(input:$input){comment{id}}}",json!({"input":{"pullRequestReviewThreadId":thread,"body":body}})).map(|_|())
    }
    pub fn resolve(&self, thread: &str, resolved: bool) -> Result<(), String> {
        let query = if resolved {
            "mutation Resolve($input:ResolveReviewThreadInput!){resolveReviewThread(input:$input){thread{id}}}"
        } else {
            "mutation Unresolve($input:UnresolveReviewThreadInput!){unresolveReviewThread(input:$input){thread{id}}}"
        };
        self.graphql(query, json!({"input":{"threadId":thread}}))
            .map(|_| ())
    }
    pub fn edit_review_comment(&self, node: &str, body: &str) -> Result<(), String> {
        self.graphql("mutation EditComment($input:UpdatePullRequestReviewCommentInput!){updatePullRequestReviewComment(input:$input){pullRequestReviewComment{id}}}",json!({"input":{"pullRequestReviewCommentId":node,"body":body}})).map(|_|())
    }
    pub fn delete_review_comment(&self, node: &str) -> Result<(), String> {
        self.graphql("mutation DeleteComment($input:DeletePullRequestReviewCommentInput!){deletePullRequestReviewComment(input:$input){clientMutationId}}",json!({"input":{"id":node}})).map(|_|())
    }
    pub fn discard_review(&self, review: &str) -> Result<(), String> {
        self.graphql("mutation DiscardReview($input:DeletePullRequestReviewInput!){deletePullRequestReview(input:$input){clientMutationId}}",json!({"input":{"pullRequestReviewId":review}})).map(|_|())
    }
    pub fn submit_review(
        &self,
        id: &RepositoryId,
        number: u64,
        review: &str,
        head: &str,
        body: &str,
        event: ReviewEvent,
    ) -> Result<(), String> {
        if self.pull(id, number)?.head.sha != head {
            return Err("This pull request has new commits. Refresh and review them before submitting your review.".into());
        }
        if event == ReviewEvent::RequestChanges && body.trim().is_empty() {
            return Err(
                "Explain the changes you're requesting before submitting this review.".into(),
            );
        }
        self.graphql("mutation SubmitReview($input:SubmitPullRequestReviewInput!){submitPullRequestReview(input:$input){pullRequestReview{id}}}",json!({"input":{"pullRequestReviewId":review,"body":body,"event":event.api()}})).map(|_|())
    }
    pub fn viewed(&self, node: &str, path: &str, viewed: bool) -> Result<(), String> {
        let query = if viewed {
            "mutation Viewed($input:MarkFileAsViewedInput!){markFileAsViewed(input:$input){pullRequest{id}}}"
        } else {
            "mutation Unviewed($input:UnmarkFileAsViewedInput!){unmarkFileAsViewed(input:$input){pullRequest{id}}}"
        };
        self.graphql(query, json!({"input":{"pullRequestId":node,"path":path}}))
            .map(|_| ())
    }
    pub fn update_branch(&self, id: &RepositoryId, number: u64, head: &str) -> Result<(), String> {
        self.request(
            "PUT",
            &format!("/repos/{}/pulls/{number}/update-branch", id.full_name()),
            Some(json!({"expected_head_sha":head})),
        )
        .map(|_| ())
    }
    pub fn merge(
        &self,
        id: &RepositoryId,
        number: u64,
        login: &str,
        input: &MergeInput,
    ) -> Result<(), String> {
        let repo = self.repository(id)?;
        let pull = self.pull(id, number)?;
        if pull.head.sha != input.expected_head_sha {
            return Err("This pull request has new commits. Review them before merging.".into());
        }
        let info = self.review_info(&pull.node_id, login)?;
        let detail = PullRequestDetail {
            pull,
            files: Vec::new(),
            files_complete: true,
            commits: Vec::new(),
            comments: Vec::new(),
            reviews: info,
            checks: Checks::default(),
        };
        if let Some(reason) = detail.merge_blocker(&repo) {
            return Err(reason);
        }
        if !repo.merge_methods().contains(&input.method) {
            return Err("That merge method is no longer enabled for this repository.".into());
        }
        let value=self.request("PUT",&format!("/repos/{}/pulls/{number}/merge",id.full_name()),Some(json!({"sha":input.expected_head_sha,"merge_method":input.method.api(),"commit_title":input.title,"commit_message":input.body})))?;
        if value["merged"] != true {
            return Err("GitHub didn't merge this pull request. Refresh its requirements before trying again.".into());
        }
        Ok(())
    }

    pub fn options(&self, id: &RepositoryId) -> Result<CreateOptions, String> {
        let root = format!("/repos/{}", id.full_name());
        let branches = self
            .paged::<Value>(&format!("{root}/branches"), None, 10000)?
            .0
            .into_iter()
            .filter_map(|v| v["name"].as_str().map(str::to_string))
            .collect();
        let labels = self.paged(&format!("{root}/labels"), None, 10000)?.0;
        let reviewers = self
            .paged(&format!("{root}/collaborators"), None, 10000)
            .map(|v| v.0)
            .unwrap_or_default();
        let teams = self
            .paged(&format!("{root}/teams"), None, 10000)
            .map(|v| v.0)
            .unwrap_or_default();
        let templates = self.templates(id)?;
        Ok(CreateOptions {
            branches,
            labels,
            reviewers,
            teams,
            templates,
        })
    }
    fn templates(&self, id: &RepositoryId) -> Result<Vec<Template>, String> {
        let data=self.graphql("query Templates($owner:String!,$name:String!){repository(owner:$owner,name:$name){pullRequestTemplates{filename body}}}",json!({"owner":id.owner,"name":id.name}))?;
        Ok(data["repository"]["pullRequestTemplates"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|t| Template {
                name: string(t, "filename"),
                body: string(t, "body"),
            })
            .collect())
    }
    pub fn blob(&self, id: &RepositoryId, sha: &str, path: &str) -> Result<Option<String>, String> {
        use base64::Engine;
        let value = self.request(
            "GET",
            &format!(
                "/repos/{}/contents/{}?ref={}",
                id.full_name(),
                path.split('/').map(encode).collect::<Vec<_>>().join("/"),
                encode(sha)
            ),
            None,
        )?;
        if value["size"].as_u64().is_none_or(|size| size > 512 * 1024)
            || value["encoding"] != "base64"
        {
            return Ok(None);
        }
        let content = string(&value, "content").replace(['\r', '\n'], "");
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(content)
            .map_err(|_| "Couldn't decode this GitHub file.".to_string())?;
        if bytes.len() > 512 * 1024 || bytes.iter().take(8192).any(|b| *b == 0) {
            return Ok(None);
        }
        Ok(String::from_utf8(bytes).ok())
    }
}

impl Default for Client {
    fn default() -> Self {
        Self::new()
    }
}
pub fn encode(text: &str) -> String {
    url::form_urlencoded::byte_serialize(text.as_bytes()).collect()
}
fn decode<T: DeserializeOwned>(bytes: &[u8]) -> Result<T, String> {
    serde_json::from_slice(bytes)
        .map_err(|_| "GitHub returned an unreadable response. Refresh to try again.".into())
}
fn from_value<T: DeserializeOwned>(value: Value) -> Result<T, String> {
    serde_json::from_value(value)
        .map_err(|_| "GitHub returned an incomplete response. Refresh to try again.".into())
}
fn string(value: &Value, key: &str) -> String {
    value[key].as_str().unwrap_or_default().to_string()
}
pub fn check_state(status: &str) -> CheckState {
    match status {
        "success" | "neutral" | "skipped" => CheckState::Passing,
        "failure" | "error" | "timed_out" | "cancelled" | "action_required" | "startup_failure"
        | "stale" => CheckState::Failing,
        _ => CheckState::Pending,
    }
}
fn summary(raw: &Value) -> PullRequest {
    let repository = |v: &Value| Repository {
        id: v["databaseId"].as_u64().unwrap_or_default(),
        name: string(v, "name"),
        owner: User {
            login: string(&v["owner"], "login"),
            ..Default::default()
        },
        ..Default::default()
    };
    PullRequest {
        number: raw["number"].as_u64().unwrap_or_default(),
        node_id: string(raw, "id"),
        title: string(raw, "title"),
        body: raw["body"].as_str().map(str::to_string),
        state: string(raw, "state").to_ascii_lowercase(),
        draft: raw["isDraft"].as_bool().unwrap_or(false),
        merged_at: raw["mergedAt"].as_str().map(str::to_string),
        html_url: string(raw, "url"),
        user: User {
            login: string(&raw["author"], "login"),
            ..Default::default()
        },
        head: BranchRef {
            name: string(raw, "headRefName"),
            sha: string(raw, "headRefOid"),
            repo: (!raw["headRepository"].is_null()).then(|| repository(&raw["headRepository"])),
            ..Default::default()
        },
        base: BranchRef {
            name: string(raw, "baseRefName"),
            sha: string(raw, "baseRefOid"),
            repo: Some(repository(&raw["repository"])),
            ..Default::default()
        },
        labels: raw["labels"]["nodes"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|l| Label {
                name: string(l, "name"),
            })
            .collect(),
        ..Default::default()
    }
}
