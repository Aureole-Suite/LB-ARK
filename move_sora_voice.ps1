if (Test-Path .\ED6_DT21.dat) {
	$scena_dir = ".\data\ED6_DT21\"
	$text_dir  = ".\data\ED6_DT22\"
} else {
	$scena_dir = ".\data\ED6_DT01\"
	$text_dir  = ".\data\ED6_DT02\"
}

New-Item -ItemType Directory -Force -Path $scena_dir
New-Item -ItemType Directory -Force -Path $text_dir

foreach ($file in Get-ChildItem .\voice\scena\*._SN) {
	 Move-Item -Path $file -Destination (Join-Path -Path $scena_dir $file.name.replace(' ', '').ToLower())
}

foreach ($file in Get-ChildItem .\voice\scena\*._DT) {
	 Move-Item -Path $file -Destination (Join-Path -Path $text_dir $file.name.replace(' ', '').ToLower())
}

Read-Host -Prompt "Press any key to continue..."
