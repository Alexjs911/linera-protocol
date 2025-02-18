// Copyright (c) Zefchain Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{collections::HashSet, time::Duration};

use anyhow::{bail, Result};
use humantime::format_duration;
use serde::Serialize;

use crate::{ci_runtime_comparison::CiRuntimeComparison, github::Github};

pub const PR_COMMENT_HEADER: &str = "## Performance Summary for commit";

#[derive(Serialize)]
pub struct PerformanceSummary {
    #[serde(skip_serializing)]
    github: Github,
    ci_runtime_comparison: CiRuntimeComparison,
}

impl PerformanceSummary {
    pub async fn init(github: Github, tracked_workflows: HashSet<String>) -> Result<Self> {
        let workflows_handler = github.workflows_handler();
        let workflows = github
            .workflows(&workflows_handler)
            .await?
            .into_iter()
            .filter(|workflow| tracked_workflows.contains(&workflow.name))
            .collect::<Vec<_>>();

        let (base_jobs, pr_jobs) = github
            .get_base_and_pr_jobs(&workflows_handler, &workflows)
            .await?;

        Ok(Self {
            github,
            ci_runtime_comparison: CiRuntimeComparison::from_jobs(base_jobs, pr_jobs)?,
        })
    }

    fn format_comment_body(&self) -> String {
        let commit_hash = self.github.context().pr_commit_hash();
        let short_commit_hash = &commit_hash[..7];
        let commit_url = format!(
            "https://github.com/{}/{}/commit/{}",
            self.github.context().repository().owner(),
            self.github.context().repository().name(),
            commit_hash
        );

        let mut markdown_content = format!(
            "{} [{}]({})\n\n",
            PR_COMMENT_HEADER, short_commit_hash, commit_url
        );

        markdown_content.push_str("### CI Runtime Comparison\n\n");
        for (workflow_name, comparisons) in self.ci_runtime_comparison.0.iter() {
            markdown_content.push_str(&format!("#### Workflow: {}\n\n", workflow_name));
            markdown_content.push_str("| Job Name | Base Runtime | PR Runtime | Runtime Difference (%) |\n");
            markdown_content.push_str("|---|---|---|---|");

            for comparison in comparisons {
                let base_runtime = format_duration(Duration::from_secs(comparison.base_runtime())).to_string();
                let pr_runtime = format_duration(Duration::from_secs(comparison.pr_runtime())).to_string();
                let runtime_difference_pct = format!("{:.2}%", comparison.runtime_difference_pct());

                markdown_content.push_str(&format!(
                    "| {} | {} | {} | {} |\n",
                    comparison.name(),
                    base_runtime,
                    pr_runtime,
                    runtime_difference_pct
                ));
            }
            markdown_content.push('\n');
        }

        markdown_content
    }

    pub async fn upsert_pr_comment(&self) -> Result<()> {
        self.github.upsert_pr_comment(self.format_comment_body()).await
    }

    async fn get_base_and_pr_jobs(
        &self,
        workflows_handler: &github::WorkflowsHandler,
        workflows: &[github::Workflow],
    ) -> Result<(Vec<github::Job>, Vec<github::Job>)> {
        let base_jobs = self
            .github
            .latest_jobs(self.github.context().base_branch(), "push", workflows_handler, workflows)
            .await?;
        if base_jobs.is_empty() {
            bail!("No base jobs found!");
        }

        let pr_jobs = self
            .github
            .latest_jobs(self.github.context().pr_branch(), "pull_request", workflows_handler, workflows)
            .await?;
        if pr_jobs.is_empty() {
            bail!("No PR jobs found!");
        }

        Ok((base_jobs, pr_jobs))
    }
}
