<?php

declare(strict_types=1);

final class CoalescedPropertyInLoop
{
    /**
     * @param iterable<int, stdClass> $rows
     * @return list<string>
     */
    public function invalid(iterable $rows): array
    {
        $bad = [];
        foreach ($rows as $row) {
            $ok = $row->item_id !== null;

            if (!$ok) {
                $bad[] = (string) ($row->item_id ?? 'none');
            }
        }

        return $bad;
    }

    public function after(iterable $rows): array
    {
        $bad = [];
        foreach ($rows as $row) {
            if ($row->item_id === null) {
                $bad[] = 'none';
            }
            $last = $row->item_id ?? 'x';
            $bad[] = $last;
        }

        return $bad;
    }
}
