---
id: intel-local-cleanup-fixture
title: Intel localhost cleanup fixture
description: Temporary isolated integration test
steps:
  - version: 2
    id: fixture-step
    type: prompt_transform
    prompt_transform.inference.provider: Custom
    prompt_transform.inference.provider.OpenAI.model: gpt-4o
    prompt_transform.inference.provider.Groq.model: llama-3.3-70b-versatile
    prompt_transform.inference.provider.Anthropic.model: claude-sonnet-4-0
    prompt_transform.inference.provider.Google.model: gemini-2.5-flash
    prompt_transform.inference.provider.OpenRouter.model: mistralai/mixtral-8x7b
    prompt_transform.inference.provider.Custom.model: fixture-model
    prompt_transform.inference.provider.Custom.baseUrl: 'http://127.0.0.1:18763/v1'
    prompt_transform.systemPromptTemplate: Return the fixture cleanup result.
    prompt_transform.userPromptTemplate: '{{input}}'
    find_replace.findText: ''
    find_replace.replaceText: ''
    find_replace.useRegex: false
createdAt: '2026-11-12T21:14:10.597Z'
updatedAt: '2026-11-12T21:14:10.597Z'
---
