use crate::executor::{ExecutionContext, ExecutionResult, SkillExecutor};
use crate::model::SkillPackage;
use crate::registry::{builtin_skills, SkillRegistry};

/// 技能引擎：顶层 API
pub struct SkillEngine {
    registry: SkillRegistry,
}

impl SkillEngine {
    /// 创建引擎并加载内置技能
    pub fn new() -> Self {
        let mut registry = SkillRegistry::new();
        for skill in builtin_skills() {
            registry.register(skill);
        }
        Self { registry }
    }

    /// 执行技能
    pub fn execute(&mut self, skill_id: &str, ctx: &ExecutionContext) -> ExecutionResult {
        SkillExecutor::execute(&mut self.registry, skill_id, ctx)
    }

    /// 列出所有技能
    pub fn list_skills(&self) -> Vec<&SkillPackage> {
        self.registry.list_all()
    }

    /// 获取技能状态
    pub fn get_skill_state(&self, id: &str) -> Option<&crate::state::SkillState> {
        self.registry.get_state(id)
    }

    /// 获取所有技能的执行统计
    pub fn get_all_stats(&self) -> Vec<(&str, &crate::state::SkillStats)> {
        let mut result = Vec::new();
        for pkg in self.registry.list_all() {
            if let Some(stats) = self.registry.get_execution_stats(&pkg.id) {
                result.push((pkg.id.as_str(), stats));
            }
        }
        result
    }
}

impl Default for SkillEngine {
    fn default() -> Self {
        Self::new()
    }
}
