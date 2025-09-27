
=== MODEL COMPARISON RESULTS ===
Test sentence: 'here is an action : is jane a good fit for this job post url : https://www.linkedin.com/jobs/view/4237328365/?alternateChannel=search&eBP=CwEAAAGZf8gRbRQYqthWdmTGt5IAkrmRYIzdFksa4Jfsjr2Na737AI_XbTSiyq8ebQcyQ52QHpvrvlznIoAiSk0bEBaoJLdOG-TIxpN5YbOlcHn0adh9vYhEPEfD60K82abXDZUNNNlf-kfgfwfzzDk2uAzHtk1uMLhNszcliVLxA2-OcOSKxZ5gqXYwz0mczVmFXSyMz02fd9aDzE71RPiXgkZ2nFwxw7kbEQXsFmxuB3BeyG1qYD8_Kh72Ni6i8aMgm7oghUGPZF1qRXCyUFheW3CeaXRCWRO9TFmioGcT295oO8R-2xZvR4atqVuo3r-lhBY0foc3kYho5uyURsZQ6_6mvM8mQfx6BrXS6M9dhk8jRd1xI2wB6SC99oBA_Ak2I_C0scID1USe3_s0BPRK2SfDcVZyRRbs7abvSC_EEel5o98USxoR3lzfY9HOmVEBEqCsoT45NbKy9uWlg8iprrs&refId=3BCyM4GbmRLDj8p8%2BtVfew%3D%3D&trackingId=Wl75W2H7UIcVefE%2BXh%2BNZw%3D%3D'
Iterations per configuration: 10

╔═ PROMPT VERSION V2 ═══════════════════════════════════════════════════════════════════════════════════════════════════════════════╗
║ ENDPOINT MATCHING
║ ├─ Cohere:
║ │  general_conversation_fallback: 10 times (100%)
║ ├─ Claude:
║ │  job-fit-analysis-1e92db66: 10 times (100%)
║ └─ DeepSeek:
║ │  job-fit-analysis-1e92db66: 10 times (100%)
║
║ PARAMETER EXTRACTION VALUES
║ ├─ job_url:
║ │  ├─ Cohere: Not found
║ │  ├─ Claude: 'https://www.linkedin.com/jobs/view/4237328365/?alternateChannel=search&eBP=CwEAAAGZf8gRbRQYqthWdmTGt5IAkrmRYIzdFksa4Jfsjr2Na737AI_XbTSiyq8ebQcyQ52QHpvrvlznIoAiSk0bEBaoJLdOG-TIxpN5YbOlcHn0adh9vYhEPEfD60K82abXDZUNNNlf-kfgfwfzzDk2uAzHtk1uMLhNszcliVLxA2-OcOSKxZ5gqXYwz0mczVmFXSyMz02fd9aDzE71RPiXgkZ2nFwxw7kbEQXsFmxuB3BeyG1qYD8_Kh72Ni6i8aMgm7oghUGPZF1qRXCyUFheW3CeaXRCWRO9TFmioGcT295oO8R-2xZvR4atqVuo3r-lhBY0foc3kYho5uyURsZQ6_6mvM8mQfx6BrXS6M9dhk8jRd1xI2wB6SC99oBA_Ak2I_C0scID1USe3_s0BPRK2SfDcVZyRRbs7abvSC_EEel5o98USxoR3lzfY9HOmVEBEqCsoT45NbKy9uWlg8iprrs&refId=3BCyM4GbmRLDj8p8%2BtVfew%3D%3D&trackingId=Wl75W2H7UIcVefE%2BXh%2BNZw%3D%3D' (100% extracted, 100% consistent)
║ │  └─ DeepSeek: 'https://www.linkedin.com/jobs/view/4237328365/?alternateChannel=search&eBP=CwEAAAGZf8gRbRQYqthWdmTGt5IAkrmRYIzdFksa4Jfsjr2Na737AI_XbTSiyq8ebQcyQ52QHpvrvlznIoAiSk0bEBaoJLdOG-TIxpN5YbOlcHn0adh9vYhEPEfD60K82abXDZUNNNlf-kfgfwfzzDk2uAzHtk1uMLhNszcliVLxA2-OcOSKxZ5gqXYwz0mczVmFXSyMz02fd9aDzE71RPiXgkZ2nFwxw7kbEQXsFmxuB3BeyG1qYD8_Kh72Ni6i8aMgm7oghUGPZF1qRXCyUFheW3CeaXRCWRO9TFmioGcT295oO8R-2xZvR4atqVuo3r-lhBY0foc3kYho5uyURsZQ6_6mvM8mQfx6BrXS6M9dhk8jRd1xI2wB6SC99oBA_Ak2I_C0scID1USe3_s0BPRK2SfDcVZyRRbs7abvSC_EEel5o98USxoR3lzfY9HOmVEBEqCsoT45NbKy9uWlg8iprrs&refId=3BCyM4GbmRLDj8p8%2BtVfew%3D%3D&trackingId=Wl75W2H7UIcVefE%2BXh%2BNZw%3D%3D' (100% extracted, 100% consistent)
║ ├─ person_name:
║ │  ├─ Cohere: Not found
║ │  ├─ Claude: 'jane' (10% extracted, 100% consistent)
║ │  └─ DeepSeek: Not extracted (0%)
║
║ PERFORMANCE
║ ├─ Response Time (ms):
║ │  ├─ Cohere: 16144ms
║ │  ├─ Claude: 22025ms
║ │  └─ DeepSeek: 52334ms
║ └─ Token Usage (in/out):
║    ├─ Cohere: 447 in / 85 out
║    ├─ Claude: 330 in / 476 out
║    └─ DeepSeek: 336 in / 485 out
╚═══════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════╝

╔═ PROMPT VERSION V3 ═══════════════════════════════════════════════════════════════════════════════════════════════════════════════╗
║ ENDPOINT MATCHING
║ ├─ Cohere:
║ │  general_conversation_fallback: 10 times (100%)
║ ├─ Claude:
║ │  job-fit-analysis-1e92db66: 10 times (100%)
║ └─ DeepSeek:
║ │  job-fit-analysis-1e92db66: 10 times (100%)
║
║ PARAMETER EXTRACTION VALUES
║ ├─ job_url:
║ │  ├─ Cohere: Not found
║ │  ├─ Claude: 'https://www.linkedin.com/jobs/view/4237328365/?alternateChannel=search&eBP=CwEAAAGZf8gRbRQYqthWdmTGt5IAkrmRYIzdFksa4Jfsjr2Na737AI_XbTSiyq8ebQcyQ52QHpvrvlznIoAiSk0bEBaoJLdOG-TIxpN5YbOlcHn0adh9vYhEPEfD60K82abXDZUNNNlf-kfgfwfzzDk2uAzHtk1uMLhNszcliVLxA2-OcOSKxZ5gqXYwz0mczVmFXSyMz02fd9aDzE71RPiXgkZ2nFwxw7kbEQXsFmxuB3BeyG1qYD8_Kh72Ni6i8aMgm7oghUGPZF1qRXCyUFheW3CeaXRCWRO9TFmioGcT295oO8R-2xZvR4atqVuo3r-lhBY0foc3kYho5uyURsZQ6_6mvM8mQfx6BrXS6M9dhk8jRd1xI2wB6SC99oBA_Ak2I_C0scID1USe3_s0BPRK2SfDcVZyRRbs7abvSC_EEel5o98USxoR3lzfY9HOmVEBEqCsoT45NbKy9uWlg8iprrs&refId=3BCyM4GbmRLDj8p8%2BtVfew%3D%3D&trackingId=Wl75W2H7UIcVefE%2BXh%2BNZw%3D%3D' (100% extracted, 100% consistent)
║ │  └─ DeepSeek: 'https://www.linkedin.com/jobs/view/4237328365/?alternateChannel=search&eBP=CwEAAAGZf8gRbRQYqthWdmTGt5IAkrmRYIzdFksa4Jfsjr2Na737AI_XbTSiyq8ebQcyQ52QHpvrvlznIoAiSk0bEBaoJLdOG-TIxpN5YbOlcHn0adh9vYhEPEfD60K82abXDZUNNNlf-kfgfwfzzDk2uAzHtk1uMLhNszcliVLxA2-OcOSKxZ5gqXYwz0mczVmFXSyMz02fd9aDzE71RPiXgkZ2nFwxw7kbEQXsFmxuB3BeyG1qYD8_Kh72Ni6i8aMgm7oghUGPZF1qRXCyUFheW3CeaXRCWRO9TFmioGcT295oO8R-2xZvR4atqVuo3r-lhBY0foc3kYho5uyURsZQ6_6mvM8mQfx6BrXS6M9dhk8jRd1xI2wB6SC99oBA_Ak2I_C0scID1USe3_s0BPRK2SfDcVZyRRbs7abvSC_EEel5o98USxoR3lzfY9HOmVEBEqCsoT45NbKy9uWlg8iprrs&refId=3BCyM4GbmRLDj8p8%2BtVfew%3D%3D&trackingId=Wl75W2H7UIcVefE%2BXh%2BNZw%3D%3D' (100% extracted, 100% consistent)
║ ├─ person_name:
║ │  ├─ Cohere: Not found
║ │  ├─ Claude: 'jane' (30% extracted, 100% consistent)
║ │  └─ DeepSeek: Not extracted (0%)
║
║ PERFORMANCE
║ ├─ Response Time (ms):
║ │  ├─ Cohere: 16150ms
║ │  ├─ Claude: 20708ms
║ │  └─ DeepSeek: 53851ms
║ └─ Token Usage (in/out):
║    ├─ Cohere: 447 in / 78 out
║    ├─ Claude: 330 in / 477 out
║    └─ DeepSeek: 336 in / 485 out
╚═══════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════╝

╔═ PROMPT VERSION V1 ═══════════════════════════════════════════════════════════════════════════════════════════════════════════════╗
║ ENDPOINT MATCHING
║ ├─ Cohere:
║ │  general_conversation_fallback: 10 times (100%)
║ ├─ Claude:
║ │  job-fit-analysis-1e92db66: 10 times (100%)
║ └─ DeepSeek:
║ │  job-fit-analysis-1e92db66: 10 times (100%)
║
║ PARAMETER EXTRACTION VALUES
║ ├─ job_url:
║ │  ├─ Cohere: Not found
║ │  ├─ Claude: 'https://www.linkedin.com/jobs/view/4237328365/?alternateChannel=search&eBP=CwEAAAGZf8gRbRQYqthWdmTGt5IAkrmRYIzdFksa4Jfsjr2Na737AI_XbTSiyq8ebQcyQ52QHpvrvlznIoAiSk0bEBaoJLdOG-TIxpN5YbOlcHn0adh9vYhEPEfD60K82abXDZUNNNlf-kfgfwfzzDk2uAzHtk1uMLhNszcliVLxA2-OcOSKxZ5gqXYwz0mczVmFXSyMz02fd9aDzE71RPiXgkZ2nFwxw7kbEQXsFmxuB3BeyG1qYD8_Kh72Ni6i8aMgm7oghUGPZF1qRXCyUFheW3CeaXRCWRO9TFmioGcT295oO8R-2xZvR4atqVuo3r-lhBY0foc3kYho5uyURsZQ6_6mvM8mQfx6BrXS6M9dhk8jRd1xI2wB6SC99oBA_Ak2I_C0scID1USe3_s0BPRK2SfDcVZyRRbs7abvSC_EEel5o98USxoR3lzfY9HOmVEBEqCsoT45NbKy9uWlg8iprrs&refId=3BCyM4GbmRLDj8p8%2BtVfew%3D%3D&trackingId=Wl75W2H7UIcVefE%2BXh%2BNZw%3D%3D' (100% extracted, 100% consistent)
║ │  └─ DeepSeek: 'https://www.linkedin.com/jobs/view/4237328365/?alternateChannel=search&eBP=CwEAAAGZf8gRbRQYqthWdmTGt5IAkrmRYIzdFksa4Jfsjr2Na737AI_XbTSiyq8ebQcyQ52QHpvrvlznIoAiSk0bEBaoJLdOG-TIxpN5YbOlcHn0adh9vYhEPEfD60K82abXDZUNNNlf-kfgfwfzzDk2uAzHtk1uMLhNszcliVLxA2-OcOSKxZ5gqXYwz0mczVmFXSyMz02fd9aDzE71RPiXgkZ2nFwxw7kbEQXsFmxuB3BeyG1qYD8_Kh72Ni6i8aMgm7oghUGPZF1qRXCyUFheW3CeaXRCWRO9TFmioGcT295oO8R-2xZvR4atqVuo3r-lhBY0foc3kYho5uyURsZQ6_6mvM8mQfx6BrXS6M9dhk8jRd1xI2wB6SC99oBA_Ak2I_C0scID1USe3_s0BPRK2SfDcVZyRRbs7abvSC_EEel5o98USxoR3lzfY9HOmVEBEqCsoT45NbKy9uWlg8iprrs&refId=3BCyM4GbmRLDj8p8%2BtVfew%3D%3D&trackingId=Wl75W2H7UIcVefE%2BXh%2BNZw%3D%3D' (100% extracted, 100% consistent)
║ ├─ person_name:
║ │  ├─ Cohere: Not found
║ │  ├─ Claude: Not extracted (0%)
║ │  └─ DeepSeek: Not extracted (0%)
║
║ PERFORMANCE
║ ├─ Response Time (ms):
║ │  ├─ Cohere: 16074ms
║ │  ├─ Claude: 19959ms
║ │  └─ DeepSeek: 52151ms
║ └─ Token Usage (in/out):
║    ├─ Cohere: 447 in / 88 out
║    ├─ Claude: 330 in / 476 out
║    └─ DeepSeek: 336 in / 485 out
╚═══════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════╝
