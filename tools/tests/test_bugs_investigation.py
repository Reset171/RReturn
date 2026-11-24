"""E2E Verification suite for investigated edge cases and regression bugs (47-82).

Modularized into:
- tools.tests.test_bugs_player: Player combat, effects, item interaction.
- tools.tests.test_bugs_world: World mechanics, entities, vehicles, audio.
"""

from .test_bugs_player import *
from .test_bugs_world import *
