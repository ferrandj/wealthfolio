//! MCP tools to manage goals (create, edit, delete, fund them) and portfolios (named groups of accounts).
//! Retirement plans and contribution limits are not here: saving them also drives host-side work (currency
//! normalisation, portfolio refresh) that the tool environment does not carry.

use serde::Deserialize;
use serde_json::json;
use wealthfolio_core::goals::{GoalFundingRuleInput, GoalUpdate, NewGoal};
use wealthfolio_core::portfolios::{NewPortfolio, PortfolioUpdate};

use super::service_tools::{fail, object, reply, service_tool};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct GoalArgs {
    goal_id: String,
}

fn goal_schema() -> serde_json::Value {
    object(json!({"goalId": {"type": "string"}}), &["goalId"])
}

service_tool!(
    CreateGoal, "create_goal", Write, additive = true,
    scopes = [FinancialPlanningWrite],
    description = "Create a goal (goalType save_up or retirement…). The goal's currency is the base currency. Fund it afterwards with save_goal_funding. A savings goal needs targetAmount.",
    schema = object(json!({
        "goalType": {"type": "string", "description": "save_up, retirement…"},
        "title": {"type": "string"},
        "description": {"type": ["string", "null"]},
        "targetAmount": {"type": ["number", "null"]},
        "priority": {"type": ["integer", "null"]},
        "startDate": {"type": ["string", "null"], "description": "YYYY-MM-DD"},
        "targetDate": {"type": ["string", "null"], "description": "YYYY-MM-DD"}
    }), &["goalType", "title"]),
    |env, args| {
        let mut goal: NewGoal = serde_json::from_value(args)?;
        goal.currency = Some(env.base_currency());
        reply(&env.goal_service().create_goal(goal).await.map_err(fail)?)
    }
);

service_tool!(
    UpdateGoal, "update_goal", Write, additive = false,
    scopes = [FinancialPlanningWrite],
    description = "Edit a goal: only the fields given change. targetDate null removes the deadline; statusLifecycle (active, achieved, archived) is how a goal is closed or archived. The goal's type cannot change.",
    schema = object(json!({
        "goalId": {"type": "string"},
        "title": {"type": "string"},
        "description": {"type": ["string", "null"]},
        "targetAmount": {"type": ["number", "null"]},
        "priority": {"type": "integer"},
        "startDate": {"type": ["string", "null"]},
        "targetDate": {"type": ["string", "null"], "description": "YYYY-MM-DD, or null to remove the deadline."},
        "statusLifecycle": {"type": "string"}
    }), &["goalId"]),
    |env, args| {
        let service = env.goal_service();
        let goal_id = args["goalId"].as_str().ok_or_else(|| {
            crate::tool::AgentToolError::InvalidInput("goalId is required".to_string())
        })?.to_string();
        let mut goal = service.get_goal(&goal_id).map_err(fail)?;
        let given = |key: &str| args.get(key).filter(|_| args.as_object().is_some_and(|m| m.contains_key(key)));
        if let Some(v) = given("title").and_then(|v| v.as_str()) { goal.title = v.to_string(); }
        if let Some(v) = given("description") { goal.description = v.as_str().map(str::to_string); }
        if let Some(v) = given("targetAmount") { goal.target_amount = v.as_f64(); }
        if let Some(v) = given("priority").and_then(|v| v.as_i64()) { goal.priority = v as i32; }
        if let Some(v) = given("startDate") { goal.start_date = v.as_str().map(str::to_string); }
        if let Some(v) = given("statusLifecycle").and_then(|v| v.as_str()) { goal.status_lifecycle = v.to_string(); }
        goal.currency = Some(env.base_currency());
        let mut update = GoalUpdate::from(goal);
        if let Some(v) = given("targetDate") {
            update.target_date = Some(v.as_str().map(str::to_string));
        }
        reply(&service.update_goal(update).await.map_err(fail)?)
    }
);

service_tool!(
    DeleteGoal,
    "delete_goal",
    Write,
    additive = false,
    scopes = [FinancialPlanningWrite],
    description = "Delete a goal with its funding and plan.",
    schema = goal_schema(),
    |env, args| {
        let args: GoalArgs = serde_json::from_value(args)?;
        let deleted = env
            .goal_service()
            .delete_goal(args.goal_id.clone())
            .await
            .map_err(fail)?;
        reply(&json!({"deleted": args.goal_id, "count": deleted}))
    }
);

service_tool!(
    GetGoalFunding,
    "get_goal_funding",
    Read,
    additive = false,
    scopes = [FinancialPlanningRead],
    description = "The accounts that fund a goal and the share of each.",
    schema = goal_schema(),
    |env, args| {
        let args: GoalArgs = serde_json::from_value(args)?;
        reply(
            &env.goal_service()
                .get_goal_funding(&args.goal_id)
                .map_err(fail)?,
        )
    }
);

service_tool!(
    GetGoalPlan,
    "get_goal_plan",
    Read,
    additive = false,
    scopes = [FinancialPlanningRead],
    description =
        "The plan stored with a goal (a retirement plan's settings and summary), or null.",
    schema = goal_schema(),
    |env, args| {
        let args: GoalArgs = serde_json::from_value(args)?;
        reply(
            &env.goal_service()
                .get_goal_plan(&args.goal_id)
                .map_err(fail)?,
        )
    }
);

service_tool!(
    SaveGoalFunding, "save_goal_funding", Write, additive = false,
    scopes = [FinancialPlanningWrite],
    description = "Replace the funding of a goal: which accounts count toward it and what share (percent) of each. An empty list removes the funding. The goal's progress is recomputed.",
    schema = object(json!({
        "goalId": {"type": "string"},
        "rules": {"type": "array", "items": object(json!({
            "accountId": {"type": "string"},
            "sharePercent": {"type": "number", "minimum": 0, "maximum": 100},
            "taxBucket": {"type": ["string", "null"]}
        }), &["accountId", "sharePercent"])}
    }), &["goalId", "rules"]),
    |env, args| {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Args { goal_id: String, rules: Vec<GoalFundingRuleInput> }
        let args: Args = serde_json::from_value(args)?;
        let service = env.goal_service();
        let saved = service.save_goal_funding(&args.goal_id, args.rules).await.map_err(fail)?;
        if let Err(error) = service.refresh_goal_summary(&args.goal_id).await {
            log::warn!("Failed to refresh goal summary after funding save for {}: {error}", args.goal_id);
        }
        reply(&saved)
    }
);

service_tool!(
    CreatePortfolio,
    "create_portfolio",
    Write,
    additive = true,
    scopes = [AccountsWrite],
    description = "Create a portfolio: a named group of accounts that reports can be scoped to.",
    schema = object(
        json!({
            "name": {"type": "string"},
            "description": {"type": ["string", "null"]},
            "sortOrder": {"type": "integer"},
            "accountIds": {"type": "array", "items": {"type": "string"}}
        }),
        &["name", "accountIds"]
    ),
    |env, args| {
        let new_portfolio: NewPortfolio = serde_json::from_value(args)?;
        reply(
            &env.portfolio_service()
                .create_portfolio(new_portfolio)
                .await
                .map_err(fail)?,
        )
    }
);

service_tool!(
    UpdatePortfolio, "update_portfolio", Write, additive = false,
    scopes = [AccountsWrite],
    description = "Replace a portfolio's name, description, order and accounts (send the complete list of accountIds to keep).",
    schema = object(json!({
        "id": {"type": "string"},
        "name": {"type": "string"},
        "description": {"type": ["string", "null"]},
        "sortOrder": {"type": "integer"},
        "accountIds": {"type": "array", "items": {"type": "string"}}
    }), &["id", "name", "accountIds"]),
    |env, args| {
        let update: PortfolioUpdate = serde_json::from_value(args)?;
        reply(&env.portfolio_service().update_portfolio(update).await.map_err(fail)?)
    }
);

service_tool!(
    DeletePortfolio,
    "delete_portfolio",
    Write,
    additive = false,
    scopes = [AccountsWrite],
    description = "Delete a portfolio (its accounts stay).",
    schema = object(json!({"id": {"type": "string"}}), &["id"]),
    |env, args| {
        #[derive(Deserialize)]
        struct Args {
            id: String,
        }
        let args: Args = serde_json::from_value(args)?;
        env.portfolio_service()
            .delete_portfolio(&args.id)
            .await
            .map_err(fail)?;
        reply(&json!({"deleted": args.id}))
    }
);
