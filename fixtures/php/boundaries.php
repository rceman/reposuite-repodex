<html>
<body>
<?php
namespace App\Boundaries;

include __DIR__ . '/included.php';
require_once 'required.php';

trait First
{
    public function shared(): void
    {
    }
}

trait Second
{
    public function shared(): void
    {
    }
}

class Combined
{
    use First, Second {
        First::shared insteadof Second;
        Second::shared as secondShared;
    }

    public function dynamic(): void
    {
        $method = 'shared';
        $this->$method();
    }
}
?>
<?php
namespace App\Boundaries\Second;

function inSecondRegion(): void
{
}
?>
</body>
</html>
