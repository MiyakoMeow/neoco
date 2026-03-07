# TECH-SKILL: Skills模块

本文档描述Neco项目的Skills模块设计。

## 1. 模块概述

Skills是轻量级、开放的格式，用于通过专业知识和工作流程来扩展AI代理的能力。

## 2. 核心概念

### 2.1 Skill定义

Skill是包含指令、脚本和资源的文件夹：

```
my-skill/
├── SKILL.md          # 必需：指令和元数据
├── scripts/         # 可选：可执行代码
├── references/      # 可选：参考资料
└── assets/          # 可选：资源文件
```

### 2.2 渐进式披露

| 阶段 | 加载内容 | 上下文消耗 |
|------|---------|-----------|
| 发现阶段 | 名称 + 描述 | ~50-100 tokens |
| 激活阶段 | 完整SKILL.md | 完整内容 |
| 执行阶段 | scripts/references | 按需 |

## 3. SKILL.md格式

```yaml
---
name: rust-coding-assistant
description: 提供Rust语言最佳实践、unsafe代码检查等能力
tags:
  - rust
  - security
---

# 技能指令内容
...
```

## 4. Skill服务

```rust
pub struct SkillService {
    skills: Arc<RwLock<HashMap<SkillId, Skill>>>,
    index: Arc<RwLock<SkillIndex>>,
}

pub struct Skill {
    pub id: SkillId,
    pub name: String,
    pub description: String,
    pub content: String,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Default)]
pub struct SkillIndex {
    pub skills: Vec<SkillInfo>,
}

pub struct SkillInfo {
    pub id: SkillId,
    pub name: String,
    pub description: String,
    pub tags: Vec<String>,
}
```

### 4.1 加载流程

```rust
impl SkillService {
    pub async fn load_index(&self) -> Result<SkillIndex, SkillError> {
        // TODO: 扫描skills目录，构建索引
        unimplemented!()
    }
    
    pub async fn load_skill(&self, id: &SkillId) -> Result<Skill, SkillError> {
        // TODO: 加载完整SKILL.md
        unimplemented!()
    }
    
    pub async fn activate(&self, id: &SkillId) -> Result<ActivatedSkill, SkillError> {
        // TODO: 激活Skill
        unimplemented!()
    }
}
```

## 5. 错误处理

```rust
#[derive(Debug, Error)]
pub enum SkillError {
    #[error("Skill未找到: {0}")]
    NotFound(SkillId),
    
    #[error("加载失败: {0}")]
    LoadFailed(String),
    
    #[error("激活失败: {0}")]
    ActivationFailed(String),
}
```

---

*关联文档：*
- [TECH.md](TECH.md) - 总体架构文档
- [TECH-TOOL.md](TECH-TOOL.md) - 工具模块
- [TECH-AGENT.md](TECH-AGENT.md) - Agent模块
